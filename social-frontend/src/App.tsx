import { Routes, Route } from "react-router-dom";
import Feed from "./features/feed/Feed";
import Post from "./features/feed/Post";
import Compose from "./features/compose/Compose";
import ComposeAlbum from "./features/compose/ComposeAlbum";
import Login from "./features/auth/Login";
import InviteAccept from "./features/auth/InviteAccept";
import Profile from "./features/profile/Profile";
import SettingsProfile from "./features/profile/SettingsProfile";
import SettingsSecurity from "./features/profile/SettingsSecurity";
import People from "./features/profile/People";
import Album from "./features/albums/Album";
import Albums from "./features/albums/Albums";
import Categories from "./features/categories/Categories";
import CookieBanner from "./components/CookieBanner";

/// The shared `<Layout>` (header + rails + main + ThemePicker) is mounted
/// per-page rather than at the App root. Each page picks whether to wrap
/// in Layout  Login and InviteAccept are intentionally chrome-light, so
/// they can opt out (or wrap a minimal variant) without an extra prop.
export default function App() {
  return (
    <>
      <Routes>
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
        <Route path="/categories" element={<Categories />} />
        <Route path="/people" element={<People />} />
        <Route path="/people/following" element={<People />} />
        <Route path="/people/followers" element={<People />} />
        <Route path="/compose-album" element={<ComposeAlbum />} />
      </Routes>
      <CookieBanner />
    </>
  );
}
