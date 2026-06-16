import { lazy } from "react";
import { Routes, Route } from "react-router-dom";
import { CookieBanner } from "@cavebatsofware/riposte-design-system/components";
import Layout from "./components/Layout";

// Page components are code-split: each lands in its own chunk fetched when its
// route is first visited (Layout, the persistent shell, stays in the entry).
// The Suspense boundary that covers their load lives in Layout, around Outlet.
const Feed = lazy(() => import("./features/feed/Feed"));
const Post = lazy(() => import("./features/feed/Post"));
const Compose = lazy(() => import("./features/compose/Compose"));
const ComposeAlbum = lazy(() => import("./features/compose/ComposeAlbum"));
const ComposeArticle = lazy(() => import("./features/compose/ComposeArticle"));
const Login = lazy(() => import("./features/auth/Login"));
const InviteAccept = lazy(() => import("./features/auth/InviteAccept"));
const Profile = lazy(() => import("./features/profile/Profile"));
const SettingsProfile = lazy(() => import("./features/profile/SettingsProfile"));
const SettingsSecurity = lazy(() => import("./features/profile/SettingsSecurity"));
const People = lazy(() => import("./features/profile/People"));
const Album = lazy(() => import("./features/albums/Album"));
const Albums = lazy(() => import("./features/albums/Albums"));
const Article = lazy(() => import("./features/articles/Article"));
const Articles = lazy(() => import("./features/articles/Articles"));
const Categories = lazy(() => import("./features/categories/Categories"));
const Contact = lazy(() => import("./features/contact/Contact"));

/// `<Layout>` (header + rails + main) is mounted as a parent layout route
/// so it persists across navigation. Page components render via Outlet
/// inside Layout; switching routes no longer remounts the BrowseRail or
/// re-fetches its categories / albums / profiles data.
export default function App() {
  return (
    <>
      <Routes>
        <Route element={<Layout />}>
          <Route path="/" element={<Feed />} />
          <Route path="/post/:id" element={<Post />} />
          <Route path="/compose" element={<Compose />} />
          <Route path="/login" element={<Login />} />
          <Route path="/invite/:code" element={<InviteAccept />} />
          <Route path="/u/:handle" element={<Profile />} />
          <Route path="/settings/profile" element={<SettingsProfile />} />
          <Route path="/settings/security" element={<SettingsSecurity />} />
          <Route path="/album/:id" element={<Album />} />
          <Route path="/albums" element={<Albums />} />
          <Route path="/articles" element={<Articles />} />
          <Route path="/articles/:id" element={<Article />} />
          <Route path="/categories" element={<Categories />} />
          <Route path="/contact" element={<Contact />} />
          <Route path="/people" element={<People />} />
          <Route path="/people/following" element={<People />} />
          <Route path="/people/followers" element={<People />} />
          <Route path="/compose-album" element={<ComposeAlbum />} />
          <Route path="/compose-article" element={<ComposeArticle />} />
        </Route>
      </Routes>
      <CookieBanner />
    </>
  );
}
