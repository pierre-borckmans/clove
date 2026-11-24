import "./style.css";
import { enhance } from "./enhance";

if (import.meta.env.DEV) {
  const { mountDev } = await import("./dev");
  mountDev();
}
enhance();
