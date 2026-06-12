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
pub use sea_orm_migration::prelude::*;

mod m20250119_000001_create_access_log;
mod m20250120_000001_drop_mac_address;
mod m20250121_000001_create_admin_users;
mod m20250122_000001_create_access_codes;
mod m20250123_000001_add_usage_count;
mod m20250124_000001_create_settings;
mod m20250125_000001_create_articles;
mod m20250127_000001_create_subscribers;
mod m20250203_000001_add_description_to_access_codes;
mod m20250210_000001_add_download_filename_to_access_codes;
mod m20250211_000001_add_last_used_at_to_access_codes;
mod m20251116_000001_add_access_log_composite_indexes;
mod m20251116_000002_create_build_status;
mod m20251120_125244_rename_count_to_tokens;
mod m20251122_000001_drop_articles_and_build_status;
mod m20251130_000001_add_totp_to_admin_users;
mod m20251130_000002_add_mfa_lockout_fields;
mod m20251202_000001_add_user_management_fields;
mod m20251202_000002_seed_site_settings;
mod m20251202_000003_add_admin_user_to_access_log;
mod m20260220_000001_add_role_to_admin_users;
mod m20260413_000001_add_subscriber_index_and_access_code_fk;
mod m20260414_000001_seed_feature_gate_settings;
mod m20260424_000001_rename_admin_users_to_users;
mod m20260425_000001_drop_user_type_column;
mod m20260426_000001_create_invite_codes;
mod m20260427_000001_add_activated_at;
mod m20260429_000001_create_posts;
mod m20260429_000002_create_post_media;
mod m20260430_000001_create_reactions;
mod m20260430_000002_create_comments;
mod m20260430_000003_create_import_jobs;
mod m20260430_000004_add_import_job_log;
mod m20260501_000001_seed_phase6_feature_gates;
mod m20260502_000001_add_user_profile_fields;
mod m20260503_000001_create_albums;
mod m20260503_000002_create_categories;
mod m20260503_000003_add_category_to_posts_albums;
mod m20260505_000001_add_locale_to_users;
mod m20260507_000001_add_posts_body_tsv;
mod m20260508_000001_add_visibility_to_categories;
mod m20260508_000002_create_category_member;
mod m20260508_000003_seed_category_mgmt_gate;
mod m20260510_000001_add_comment_edited_at;
mod m20260510_000002_create_comment_reactions;
mod m20260510_000003_create_follows;
mod m20260512_000001_add_kind_slug_to_posts;
mod m20260512_000002_migrate_albums_to_posts;
mod m20260512_000003_migrate_album_media_to_post_media;
mod m20260512_000004_drop_albums_tables;
mod m20260512_000005_create_post_media_reaction;
mod m20260512_000006_create_post_media_comment;
mod m20260514_000001_drop_tsvector_search;
mod m20260514_000002_add_bm25_search;
mod m20260519_000001_add_thumbnail_icon_to_post_media;
mod m20260519_000002_add_avatar_icon_to_users;
mod m20260520_000001_seed_max_image_dimension;
mod m20260520_000002_add_encrypted_to_settings;
mod m20260522_000001_create_article_details;
mod m20260522_000002_extend_bm25_with_slug;
mod m20260524_000001_add_post_media_ordinal_unique;
#[cfg(feature = "business")]
mod m20260606_000001_create_orders;
#[cfg(feature = "business")]
mod m20260606_000002_seed_business_settings;
#[cfg(feature = "business")]
mod m20260606_000003_seed_business_secrets;
#[cfg(feature = "business")]
mod m20260606_000004_seed_shop_url;
#[cfg(feature = "business")]
mod m20260606_000005_seed_order_statuses;
#[cfg(feature = "business")]
mod m20260608_000001_create_phone_verification;
#[cfg(feature = "business")]
mod m20260608_000002_add_phone_verification_to_orders;
#[cfg(feature = "business")]
mod m20260608_000003_seed_twilio_settings;
mod m20260610_000001_seed_email_provider_settings;

pub struct Migrator;

impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20250119_000001_create_access_log::Migration),
            Box::new(m20250120_000001_drop_mac_address::Migration),
            Box::new(m20250121_000001_create_admin_users::Migration),
            Box::new(m20250122_000001_create_access_codes::Migration),
            Box::new(m20250123_000001_add_usage_count::Migration),
            Box::new(m20250124_000001_create_settings::Migration),
            Box::new(m20250125_000001_create_articles::Migration),
            Box::new(m20250127_000001_create_subscribers::Migration),
            Box::new(m20250203_000001_add_description_to_access_codes::Migration),
            Box::new(m20250210_000001_add_download_filename_to_access_codes::Migration),
            Box::new(m20250211_000001_add_last_used_at_to_access_codes::Migration),
            Box::new(m20251116_000001_add_access_log_composite_indexes::Migration),
            Box::new(m20251116_000002_create_build_status::Migration),
            Box::new(m20251120_125244_rename_count_to_tokens::Migration),
            Box::new(m20251122_000001_drop_articles_and_build_status::Migration),
            Box::new(m20251130_000001_add_totp_to_admin_users::Migration),
            Box::new(m20251130_000002_add_mfa_lockout_fields::Migration),
            Box::new(m20251202_000001_add_user_management_fields::Migration),
            Box::new(m20251202_000002_seed_site_settings::Migration),
            Box::new(m20251202_000003_add_admin_user_to_access_log::Migration),
            Box::new(m20260220_000001_add_role_to_admin_users::Migration),
            Box::new(m20260413_000001_add_subscriber_index_and_access_code_fk::Migration),
            Box::new(m20260414_000001_seed_feature_gate_settings::Migration),
            Box::new(m20260424_000001_rename_admin_users_to_users::Migration),
            Box::new(m20260425_000001_drop_user_type_column::Migration),
            Box::new(m20260426_000001_create_invite_codes::Migration),
            Box::new(m20260427_000001_add_activated_at::Migration),
            Box::new(m20260429_000001_create_posts::Migration),
            Box::new(m20260429_000002_create_post_media::Migration),
            Box::new(m20260430_000001_create_reactions::Migration),
            Box::new(m20260430_000002_create_comments::Migration),
            Box::new(m20260430_000003_create_import_jobs::Migration),
            Box::new(m20260430_000004_add_import_job_log::Migration),
            Box::new(m20260501_000001_seed_phase6_feature_gates::Migration),
            Box::new(m20260502_000001_add_user_profile_fields::Migration),
            Box::new(m20260503_000001_create_albums::Migration),
            Box::new(m20260503_000002_create_categories::Migration),
            Box::new(m20260503_000003_add_category_to_posts_albums::Migration),
            Box::new(m20260505_000001_add_locale_to_users::Migration),
            Box::new(m20260507_000001_add_posts_body_tsv::Migration),
            Box::new(m20260508_000001_add_visibility_to_categories::Migration),
            Box::new(m20260508_000002_create_category_member::Migration),
            Box::new(m20260508_000003_seed_category_mgmt_gate::Migration),
            Box::new(m20260510_000001_add_comment_edited_at::Migration),
            Box::new(m20260510_000002_create_comment_reactions::Migration),
            Box::new(m20260510_000003_create_follows::Migration),
            Box::new(m20260512_000001_add_kind_slug_to_posts::Migration),
            Box::new(m20260512_000002_migrate_albums_to_posts::Migration),
            Box::new(m20260512_000003_migrate_album_media_to_post_media::Migration),
            Box::new(m20260512_000004_drop_albums_tables::Migration),
            Box::new(m20260512_000005_create_post_media_reaction::Migration),
            Box::new(m20260512_000006_create_post_media_comment::Migration),
            Box::new(m20260514_000001_drop_tsvector_search::Migration),
            Box::new(m20260514_000002_add_bm25_search::Migration),
            Box::new(m20260519_000001_add_thumbnail_icon_to_post_media::Migration),
            Box::new(m20260519_000002_add_avatar_icon_to_users::Migration),
            Box::new(m20260520_000001_seed_max_image_dimension::Migration),
            Box::new(m20260520_000002_add_encrypted_to_settings::Migration),
            Box::new(m20260522_000001_create_article_details::Migration),
            Box::new(m20260522_000002_extend_bm25_with_slug::Migration),
            Box::new(m20260524_000001_add_post_media_ordinal_unique::Migration),
            #[cfg(feature = "business")]
            Box::new(m20260606_000001_create_orders::Migration),
            #[cfg(feature = "business")]
            Box::new(m20260606_000002_seed_business_settings::Migration),
            #[cfg(feature = "business")]
            Box::new(m20260606_000003_seed_business_secrets::Migration),
            #[cfg(feature = "business")]
            Box::new(m20260606_000004_seed_shop_url::Migration),
            #[cfg(feature = "business")]
            Box::new(m20260606_000005_seed_order_statuses::Migration),
            #[cfg(feature = "business")]
            Box::new(m20260608_000001_create_phone_verification::Migration),
            #[cfg(feature = "business")]
            Box::new(m20260608_000002_add_phone_verification_to_orders::Migration),
            #[cfg(feature = "business")]
            Box::new(m20260608_000003_seed_twilio_settings::Migration),
            Box::new(m20260610_000001_seed_email_provider_settings::Migration),
        ]
    }
}
