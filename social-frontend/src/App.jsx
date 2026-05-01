import { Routes, Route } from "react-router-dom";
import Feed from "./pages/Feed";
import Post from "./pages/Post";
import Compose from "./pages/Compose";
import Login from "./pages/Login";
import InviteAccept from "./pages/InviteAccept";
import Profile from "./pages/Profile";
import SettingsProfile from "./pages/SettingsProfile";
import SettingsSecurity from "./pages/SettingsSecurity";
import Album from "./pages/Album";
import ComposeAlbum from "./pages/ComposeAlbum";
import CookieBanner from "./components/CookieBanner";

/// The shared `<Layout>` (header + rails + main + ThemePicker) is mounted
/// per-page rather than at the App root. Each page picks whether to wrap
/// in Layout — Login and InviteAccept are intentionally chrome-light, so
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
        <Route path="/compose-album" element={<ComposeAlbum />} />
      </Routes>
      <CookieBanner />
    </>
  );
}
