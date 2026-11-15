import type { EquipmentStatus, ReservationStatus, Role } from "./api";

export function intervalsOverlap(startA: number, endA: number, startB: number, endB: number): boolean {
  return startA < endB && startB < endA;
}

export function canCancel(status: ReservationStatus, role: Role, isOwner: boolean): boolean {
  return (role === "admin" || isOwner) && (status === "pending" || status === "approved");
}

export function nextActions(status: ReservationStatus, role: Role): string[] {
  if (role !== "admin") return [];
  switch (status) {
    case "pending": return ["approve", "reject"];
    case "approved": return ["checkout"];
    case "checked_out": return ["return"];
    default: return [];
  }
}

export function equipmentTone(status: EquipmentStatus): "ready" | "paused" {
  return status === "active" ? "ready" : "paused";
}

export function localDateTime(date: Date): string {
  const local = new Date(date.getTime() - date.getTimezoneOffset() * 60_000);
  return local.toISOString().slice(0, 16);
}
