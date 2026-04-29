import { Routes, Route } from "react-router-dom";
import Feed from "./pages/Feed";
import Post from "./pages/Post";
import Compose from "./pages/Compose";
import Login from "./pages/Login";
import InviteAccept from "./pages/InviteAccept";
import CookieBanner from "./components/CookieBanner";
import ThemePicker from "./components/ThemePicker";

export default function App() {
  return (
    <>
      <Routes>
        <Route path="/" element={<Feed />} />
        <Route path="/post/:id" element={<Post />} />
        <Route path="/compose" element={<Compose />} />
        <Route path="/login" element={<Login />} />
        <Route path="/invite/:code" element={<InviteAccept />} />
      </Routes>
      <ThemePicker />
      <CookieBanner />
    </>
  );
}
