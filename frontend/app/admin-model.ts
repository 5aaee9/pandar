import { useFormatter } from "next-intl";

import type { User } from "./dashboard-types";

export const roles: User["role"][] = ["tenant_admin", "operator", "viewer"];

export function useAdminDate() {
  const format = useFormatter();
  return (value: string) => {
    const d = new Date(value);
    if (Number.isNaN(d.getTime())) return value;
    return format.dateTime(d, {
      dateStyle: "medium",
      timeStyle: "short",
      timeZone: "UTC",
    });
  };
}
