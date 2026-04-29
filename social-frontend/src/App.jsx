import { Routes, Route } from "react-router-dom";
import Feed from "./pages/Feed";
import Login from "./pages/Login";
import InviteAccept from "./pages/InviteAccept";
import CookieBanner from "./components/CookieBanner";

export default function App() {
  return (
    <>
      <Routes>
        <Route path="/" element={<Feed />} />
        <Route path="/login" element={<Login />} />
        <Route path="/invite/:code" element={<InviteAccept />} />
      </Routes>
      <CookieBanner />
    </>
  );
}
