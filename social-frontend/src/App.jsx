import { Routes, Route } from "react-router-dom";
import Feed from "./pages/Feed";

export default function App() {
  return (
    <Routes>
      <Route path="/" element={<Feed />} />
    </Routes>
  );
}
