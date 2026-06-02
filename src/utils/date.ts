import type { DashboardRange } from "../types/dashboard";

export function toLocalDateString(date: Date) {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

export function getLocalDateWindow(range: DashboardRange) {
  const to = new Date();
  const from = new Date(to);
  if (range === "7d") {
    from.setDate(to.getDate() - 6);
  }
  if (range === "30d") {
    from.setDate(to.getDate() - 29);
  }
  if (range === "90d") {
    from.setDate(to.getDate() - 89);
  }

  return {
    from: toLocalDateString(from),
    to: toLocalDateString(to),
  };
}
