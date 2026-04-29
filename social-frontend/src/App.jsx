import { Routes, Route } from "react-router-dom";
import Feed from "./pages/Feed";
import Login from "./pages/Login";

export default function App() {
  return (
    <Routes>
      <Route path="/" element={<Feed />} />
      <Route path="/login" element={<Login />} />
    </Routes>
  );
}
