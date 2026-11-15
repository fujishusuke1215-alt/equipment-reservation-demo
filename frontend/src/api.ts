export type Role = "member" | "admin";
export type EquipmentStatus = "active" | "maintenance" | "retired";
export type ReservationStatus =
  | "pending"
  | "approved"
  | "checked_out"
  | "returned"
  | "rejected"
  | "cancelled";

export interface Person {
  id: string;
  display_name: string;
  role: Role;
}

export interface Equipment {
  id: string;
  asset_tag: string;
  name: string;
  category: string;
  description: string;
  location: string;
  status: EquipmentStatus;
  created_at: string;
  updated_at: string;
}

export interface Reservation {
  id: string;
  equipment_id: string;
  user_id: string;
  starts_at: string;
  ends_at: string;
  purpose: string;
  status: ReservationStatus;
  version: number;
  created_at: string;
  updated_at: string;
  equipment_name: string;
  asset_tag: string;
  requester_name: string;
}

export interface BusySlot {
  starts_at: string;
  ends_at: string;
}

export interface AuditEvent {
  id: number;
  entity_type: "equipment" | "reservation";
  entity_id: string;
  actor_user_id: string;
  actor_name: string;
  action: string;
  from_state: string | null;
  to_state: string | null;
  occurred_at: string;
}

export interface EquipmentInput {
  asset_tag: string;
  name: string;
  category: string;
  description: string;
  location: string;
}

export interface EquipmentUpdate extends EquipmentInput {
  status: EquipmentStatus;
}

interface ProblemDetails {
  code?: string;
  detail?: string;
  title?: string;
}

export class ApiProblem extends Error {
  readonly code: string;
  constructor(message: string, code = "request_failed") {
    super(message);
    this.name = "ApiProblem";
    this.code = code;
  }
}

async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
  const response = await fetch(path, {
    ...init,
    credentials: "same-origin",
    headers: {
      ...(init.body ? { "Content-Type": "application/json" } : {}),
      ...init.headers,
    },
  });
  if (!response.ok) {
    const problem = (await response.json().catch(() => ({}))) as ProblemDetails;
    throw new ApiProblem(
      problem.detail ?? problem.title ?? "Request failed (" + response.status + ")",
      problem.code,
    );
  }
  if (response.status === 204) return undefined as T;
  return (await response.json()) as T;
}

export const api = {
  session: () => request<{ user: Person | null }>("/api/auth/session"),
  login: (role: Role) =>
    request<{ user: Person }>("/api/auth/dev-login", {
      method: "POST",
      body: JSON.stringify({ role }),
    }),
  logout: () => request<void>("/api/auth/logout", { method: "POST" }),
  equipment: () => request<Equipment[]>("/api/equipment"),
  availability: (id: string, from: string, to: string) =>
    request<{ busy_slots: BusySlot[] }>(
      "/api/equipment/" + id + "/availability?from=" + encodeURIComponent(from) + "&to=" + encodeURIComponent(to),
    ),
  createEquipment: (input: EquipmentInput) =>
    request<Equipment>("/api/equipment", {
      method: "POST",
      body: JSON.stringify(input),
    }),
  updateEquipment: (id: string, input: EquipmentUpdate) =>
    request<Equipment>("/api/equipment/" + id, {
      method: "PATCH",
      body: JSON.stringify(input),
    }),
  reservations: (all: boolean) =>
    request<Reservation[]>("/api/reservations" + (all ? "?scope=all" : "")),
  reserve: (input: {
    equipment_id: string;
    starts_at: string;
    ends_at: string;
    purpose: string;
  }) =>
    request<Reservation>("/api/reservations", {
      method: "POST",
      headers: { "Idempotency-Key": crypto.randomUUID() },
      body: JSON.stringify(input),
    }),
  transition: (id: string, action: string, note = "") =>
    request<Reservation>("/api/reservations/" + id + "/" + action, {
      method: "POST",
      body: JSON.stringify({ note }),
    }),
  audit: () => request<AuditEvent[]>("/api/audit"),
};
