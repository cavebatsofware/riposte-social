/*  This file is part of riposte-social
 *  Copyright (C) 2026 Grant DeFayette
 *
 *  riposte-social is free software: you can redistribute it and/or modify
 *  it under the terms of the GNU General Public License as published by
 *  the Free Software Foundation, version 3 of the License (GPL-3.0-only).
 *
 *  riposte-social is distributed in the hope that it will be useful,
 *  but WITHOUT ANY WARRANTY; without even the implied warranty of
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *  GNU General Public License for more details.
 *
 *  You should have received a copy of the GNU General Public License
 *  along with riposte-social.  If not, see <https://www.gnu.org/licenses/gpl-3.0.html>.
 */
//! Single source of truth for visibility checks.
//!
//! Visibility lives in two places:
//! - `posts.visibility` and `albums.visibility` carry one of `public` /
//!   `commenters` / `posters` / `private` for *uncategorized* rows.
//! - `categories.visibility` carries any of those four PLUS a fifth tier
//!   `user_list` whose membership lives in the `category_member` join
//!   table. When a post or album has a `category_id`, the parent
//!   category's visibility is authoritative; the row's own visibility
//!   column is preserved on disk but ignored at read time.
//!
//! `ViewerCtx::build` runs a single subquery up-front to produce the set
//! of category ids the caller can see (covering both role-tier and
//! `user_list` ACL hits). Feed queries then add one SQL clause:
//! `(category_id IS NULL AND <uncategorized predicate>) OR
//!  category_id IN (:accessible_category_ids)`. No per-row category
//! lookup happens during pagination.

use crate::admin::UserAuth;
use crate::entities::{category, category_member, post, user, Category, CategoryMember, Post};
use crate::errors::{AppError, AppResult};
use crate::middleware::admin_auth::UserAuthSession;
use anyhow::Result;
use sea_orm::{
    ColumnTrait, ConnectionTrait, EntityTrait, FromQueryResult, QueryFilter, QuerySelect,
    QueryTrait,
};
use uuid::Uuid;

/// Slim subset of `post` columns needed to evaluate visibility. Used by
/// hot read paths (per-media engagement, lightbox lazy loads) so the
/// SELECT excludes `body` and the `body_tsv` tsvector that together can
/// add several KB per row of network bytes per visibility check.
#[derive(Debug, Clone, FromQueryResult)]
pub struct PostVisibilityCols {
    pub id: Uuid,
    pub author_id: Uuid,
    pub visibility: String,
    pub category_id: Option<Uuid>,
}

/// Fifth visibility tier used only on categories. The `category_member`
/// table enumerates the user ids granted access.
pub const VISIBILITY_USER_LIST: &str = "user_list";

/// Validate a tier accepted on a category row. Categories accept all
/// post-level tiers plus `user_list`.
pub fn is_valid_category_visibility(visibility: &str) -> bool {
    post::is_valid_visibility(visibility) || visibility == VISIBILITY_USER_LIST
}

/// Caller's effective visibility tier for the feed query. Maps the user's
/// role to the set of visibility values the caller is permitted to see by
/// role alone. `private` is handled separately by [`can_read_post`]
/// since it's author-scoped, not role-scoped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedTier {
    /// Anonymous visitors. Public posts only.
    Anonymous,
    /// Commenters see public + commenter-only posts.
    Commenter,
    /// Posters and administrators see all role-gated tiers.
    Privileged,
}

impl FeedTier {
    pub fn from_role(role: Option<&str>) -> Self {
        match role {
            Some(user::ROLE_ADMINISTRATOR) | Some(user::ROLE_POSTER) => FeedTier::Privileged,
            Some(user::ROLE_COMMENTER) => FeedTier::Commenter,
            _ => FeedTier::Anonymous,
        }
    }

    /// Visibility values this tier is permitted to read by role. Excludes
    /// `private` because that tier is author-scoped (use [`can_read_post`]
    /// for the full check).
    pub fn allowed_visibilities(self) -> &'static [&'static str] {
        match self {
            FeedTier::Anonymous => &[post::VISIBILITY_PUBLIC],
            FeedTier::Commenter => &[post::VISIBILITY_PUBLIC, post::VISIBILITY_COMMENTERS],
            FeedTier::Privileged => &[
                post::VISIBILITY_PUBLIC,
                post::VISIBILITY_COMMENTERS,
                post::VISIBILITY_POSTERS,
            ],
        }
    }

    /// Role-only check. Returns true when the tier may read the visibility
    /// purely by role. Always returns false for `private` because that
    /// tier requires the author-id check that this method doesn't have.
    pub fn can_read(self, visibility: &str) -> bool {
        self.allowed_visibilities().contains(&visibility)
    }
}

/// Full uncategorized-row visibility check. Returns true when:
/// - the row's visibility is one of the role-allowed tiers, OR
/// - the row is `private` and `viewer_id` is the author.
///
/// Anonymous viewers (`viewer_id = None`) can never read `private` rows.
/// Admins are NOT a special case for uncategorized private content:
/// they cannot read other users' private rows via feed or listing
/// surfaces. Admin moderation against a known id still works through
/// dedicated routes that operate without the visibility filter.
pub fn can_read_post(
    tier: FeedTier,
    visibility: &str,
    author_id: Uuid,
    viewer_id: Option<Uuid>,
) -> bool {
    if visibility == post::VISIBILITY_PRIVATE {
        return viewer_id == Some(author_id);
    }
    if tier.can_read(visibility) {
        return true;
    }
    false
}

/// Per-request visibility context. Computed once from the auth session;
/// reused by both feed-query predicates and single-row checks so the same
/// rule fires everywhere.
#[derive(Clone, Debug)]
pub struct ViewerCtx {
    pub tier: FeedTier,
    pub viewer_id: Option<Uuid>,
    /// Caller's role string (`administrator` / `poster` / `commenter`),
    /// when authenticated. Lets the per-request set distinguish admin
    /// from poster (both fall under `FeedTier::Privileged`) so the
    /// admin-moderation short-circuit fires only for actual admins.
    pub role: Option<String>,
    /// Category ids the viewer can read into. Includes role-matched
    /// categories, `user_list` categories the viewer is a member of, and
    /// categories the viewer created. For administrators this is every
    /// category id (moderation access).
    pub accessible_category_ids: Vec<Uuid>,
}

impl ViewerCtx {
    /// Build a context from the current auth session. Runs the
    /// category-resolution subquery once so the resulting set can be
    /// reused as a single SQL predicate by every read path in this
    /// request.
    pub async fn build<C>(db: &C, auth_session: &UserAuthSession) -> Result<Self>
    where
        C: ConnectionTrait,
    {
        let user = auth_session.user().await;
        let tier = FeedTier::from_role(user.as_ref().map(|u| u.role.as_str()));
        let viewer_id = user.as_ref().map(|u| u.id);
        let role = user.as_ref().map(|u| u.role.clone());
        let accessible_category_ids =
            resolve_accessible_categories(db, tier, viewer_id, role.as_deref()).await?;
        Ok(Self {
            tier,
            viewer_id,
            role,
            accessible_category_ids,
        })
    }

    /// Build directly from an authenticated `UserAuth`. Used by write
    /// paths that already extracted the user (engagement, compose). Runs
    /// the same category-resolution subquery as `build`.
    pub async fn from_user_auth_async<C>(db: &C, user: &UserAuth) -> Result<Self>
    where
        C: ConnectionTrait,
    {
        let tier = FeedTier::from_role(Some(user.role.as_str()));
        let viewer_id = Some(user.id);
        let role = Some(user.role.clone());
        let accessible_category_ids =
            resolve_accessible_categories(db, tier, viewer_id, role.as_deref()).await?;
        Ok(Self {
            tier,
            viewer_id,
            role,
            accessible_category_ids,
        })
    }

    /// Build the context for an anonymous viewer without a session.
    /// Used by always-unauthenticated surfaces (sitemap generation) so
    /// they share the exact predicate the public API serves: public
    /// tier, and an accessible set containing only public categories.
    pub async fn anonymous<C>(db: &C) -> Result<Self>
    where
        C: ConnectionTrait,
    {
        let accessible_category_ids =
            resolve_accessible_categories(db, FeedTier::Anonymous, None, None).await?;
        Ok(Self {
            tier: FeedTier::Anonymous,
            viewer_id: None,
            role: None,
            accessible_category_ids,
        })
    }

    /// Synchronous build with no category subquery. Use only when the
    /// caller has confirmed it doesn't need category-driven access (e.g.
    /// a single-row read that already has both the row and its category
    /// in hand). Most paths should use `from_user_auth_async`.
    pub fn from_user_auth(user: &UserAuth) -> Self {
        Self {
            tier: FeedTier::from_role(Some(user.role.as_str())),
            viewer_id: Some(user.id),
            role: Some(user.role.clone()),
            accessible_category_ids: Vec::new(),
        }
    }

    /// True when the caller has the administrator role. Used to short-
    /// circuit category-visibility checks (admins moderate at the
    /// category layer; uncategorized per-post privacy is unaffected).
    fn is_admin(&self) -> bool {
        self.role.as_deref() == Some(user::ROLE_ADMINISTRATOR)
    }

    /// SQL predicate for a feed-style listing. Generic over the column
    /// types so both `posts.*` and `albums.*` columns plug in.
    ///
    /// Inheritance shape:
    /// ```text
    /// (category_id IS NULL
    ///    AND (visibility IN allowed
    ///         OR (visibility = 'private' AND author_id = viewer)))
    /// OR (category_id IS NOT NULL
    ///     AND (author_id = viewer
    ///          OR category_id IN (:accessible_category_ids)))
    /// ```
    /// Categorized rows defer to the category set OR the post's author
    /// the author-override safety net keeps a row visible to whoever
    /// wrote it (covers admin-into-someone-else's-cat and demoted-admin
    /// edge cases). Uncategorized rows fall back to the row's own
    /// visibility column.
    pub fn feed_condition<V, A, C>(
        &self,
        visibility_col: V,
        author_col: A,
        category_col: C,
    ) -> sea_orm::Condition
    where
        V: ColumnTrait,
        A: ColumnTrait,
        C: ColumnTrait,
    {
        // Uncategorized branch: per-row visibility predicate.
        let allowed: Vec<&'static str> = self.tier.allowed_visibilities().to_vec();
        let mut uncategorized_vis = sea_orm::Condition::any().add(visibility_col.is_in(allowed));
        if let Some(vid) = self.viewer_id {
            uncategorized_vis = uncategorized_vis.add(
                sea_orm::Condition::all()
                    .add(visibility_col.eq(post::VISIBILITY_PRIVATE))
                    .add(author_col.eq(vid)),
            );
        }
        let uncategorized_branch = sea_orm::Condition::all()
            .add(category_col.is_null())
            .add(uncategorized_vis);

        // Categorized branch: author-override OR category-in-accessible.
        // Author-override comes first (cheaper indexed equality on a
        // Uuid column vs. an IN-list scan); applies only when the caller
        // is authenticated.
        let mut categorized_inner = sea_orm::Condition::any();
        if let Some(vid) = self.viewer_id {
            categorized_inner = categorized_inner.add(author_col.eq(vid));
        }
        if !self.accessible_category_ids.is_empty() {
            categorized_inner =
                categorized_inner.add(category_col.is_in(self.accessible_category_ids.clone()));
        }
        // If neither branch added a condition (anonymous viewer, empty
        // accessible set), fall back to an always-false predicate so the
        // OR doesn't widen by accident.
        let categorized_inner =
            if self.viewer_id.is_none() && self.accessible_category_ids.is_empty() {
                sea_orm::Condition::all().add(
                    category_col
                        .eq(Uuid::nil())
                        .and(category_col.ne(Uuid::nil())),
                )
            } else {
                categorized_inner
            };
        let categorized_branch = sea_orm::Condition::all()
            .add(category_col.is_not_null())
            .add(categorized_inner);

        sea_orm::Condition::any()
            .add(uncategorized_branch)
            .add(categorized_branch)
    }

    /// When the resource has a category, the category's visibility wins
    /// and the resource's own `visibility` is ignored. Author short-
    /// circuit fires first so an author can always read their own work.
    pub fn permits_read(
        &self,
        author_id: Uuid,
        visibility: &str,
        category: Option<&category::Model>,
    ) -> bool {
        if let Some(cat) = category {
            if self.viewer_id == Some(author_id) {
                return true;
            }
            return self.can_view_category(cat);
        }
        can_read_post(self.tier, visibility, author_id, self.viewer_id)
    }

    /// Whether this viewer can read into the given category. Admins
    /// always can (moderation access); otherwise the accessible set is
    /// the source of truth.
    pub fn can_view_category(&self, cat: &category::Model) -> bool {
        if self.is_admin() {
            return true;
        }
        self.accessible_category_ids.contains(&cat.id)
    }
}

/// Resolve the set of category ids visible to this caller.
///
/// Three OR'd predicates in a single SELECT (selecting from `categories`
/// directly, so each matching row is returned once, no dedup needed):
/// 1. Categories the viewer created  any tier, since "I created it, I
///    see it" (only when authenticated).
/// 2. Categories whose `visibility` is in the role's allowed list.
/// 3. Categories whose `visibility = 'user_list'` AND the viewer is in
///    `category_member` (only when authenticated), via a subquery.
///
/// Administrators short-circuit to "every category id" so they can
/// moderate at the category layer. Anonymous viewers get predicate 2 only
/// (no `viewer_id`). The result is used by `feed_condition` and
/// `can_view_category`. Runs once per request.
async fn resolve_accessible_categories<C>(
    db: &C,
    tier: FeedTier,
    viewer_id: Option<Uuid>,
    role: Option<&str>,
) -> Result<Vec<Uuid>>
where
    C: ConnectionTrait,
{
    // Admin short-circuit: every category is readable for moderation.
    // Uncategorized per-post `private` is unaffected (governed by
    // `can_read_post`).
    if role == Some(user::ROLE_ADMINISTRATOR) {
        return Ok(Category::find()
            .select_only()
            .column(category::Column::Id)
            .into_tuple::<Uuid>()
            .all(db)
            .await?);
    }

    let allowed: Vec<&'static str> = tier.allowed_visibilities().to_vec();

    // Predicate 2 (role-based) always applies. Predicates 1 (creator) and 3
    // (user_list membership) only when authenticated.
    let mut cond = sea_orm::Condition::any().add(category::Column::Visibility.is_in(allowed));
    if let Some(vid) = viewer_id {
        cond = cond.add(category::Column::CreatedBy.eq(vid));

        let member_categories = CategoryMember::find()
            .select_only()
            .column(category_member::Column::CategoryId)
            .filter(category_member::Column::UserId.eq(vid))
            .into_query();
        cond = cond.add(
            sea_orm::Condition::all()
                .add(category::Column::Visibility.eq(VISIBILITY_USER_LIST))
                .add(category::Column::Id.in_subquery(member_categories)),
        );
    }

    Ok(Category::find()
        .filter(cond)
        .select_only()
        .column(category::Column::Id)
        .into_tuple::<Uuid>()
        .all(db)
        .await?)
}

/// Compose-time gate: confirm `user` may write a post or album into
/// `category`. Admins always pass. For non-admins:
///   - public/commenters/posters categories: existing role gate is
///     sufficient (the route layer already enforces admin/poster), no
///     extra check.
///   - `user_list` categories: the author must be in `category_member`.
///   - private categories: only the category creator may compose
///     (private would be a strange tier on a category, but the rule is
///     consistent with the single-author semantics on posts).
///
/// Returns 403 with a generic message when blocked. The category
/// existence check happens at the call site before this helper runs.
pub async fn ensure_can_compose_into_category<C>(
    db: &C,
    user: &UserAuth,
    cat: &category::Model,
) -> AppResult<()>
where
    C: ConnectionTrait,
{
    if user.role == user::ROLE_ADMINISTRATOR {
        return Ok(());
    }
    match cat.visibility.as_str() {
        VISIBILITY_USER_LIST => {
            let is_member = CategoryMember::find()
                .filter(category_member::Column::CategoryId.eq(cat.id))
                .filter(category_member::Column::UserId.eq(user.id))
                .one(db)
                .await?
                .is_some();
            if !is_member {
                return Err(AppError::Forbidden(
                    "You aren't a member of that category".to_string(),
                ));
            }
            Ok(())
        }
        v if v == post::VISIBILITY_PRIVATE => {
            if cat.created_by == Some(user.id) {
                Ok(())
            } else {
                Err(AppError::Forbidden(
                    "Only the category owner may post into a private category".to_string(),
                ))
            }
        }
        _ => Ok(()),
    }
}

/// Load a live (non-deleted) post if the caller can read it. Returns the
/// same "Post not found" error for both missing rows and under-tier calls
/// so existence isn't disclosed. Used by write paths (engagement,
/// compose-time checks) that already extract a `UserAuth`.
///
/// Loads the parent category (when set) so the visibility check honors
/// category-driven access. One extra round trip per write path; tolerable
/// at engagement scale.
pub async fn load_visible_post<C>(db: &C, post_id: Uuid, user: &UserAuth) -> AppResult<post::Model>
where
    C: ConnectionTrait,
{
    let parent = Post::find_by_id(post_id)
        .filter(post::Column::DeletedAt.is_null())
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound("Post not found".to_string()))?;

    let cat = if let Some(cid) = parent.category_id {
        Category::find_by_id(cid).one(db).await?
    } else {
        None
    };

    let ctx = ViewerCtx::from_user_auth_async(db, user)
        .await
        .map_err(|e| AppError::InternalError(format!("viewer ctx: {:#}", e)))?;
    if !ctx.permits_read(parent.author_id, &parent.visibility, cat.as_ref()) {
        return Err(AppError::NotFound("Post not found".to_string()));
    }
    Ok(parent)
}
