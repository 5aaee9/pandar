"use client";

import { useEffect, useState } from "react";
import { useTranslations } from "next-intl";

import type { InviteStatus } from "./users-model";
import { userInitials } from "./users-model";

export function UserAvatar({
  name,
  size = "md",
}: {
  name: string;
  size?: "md" | "lg";
}) {
  const sizeClass = size === "lg" ? "size-10 text-sm" : "size-8 text-xs";
  return (
    <span
      aria-hidden="true"
      className={`inline-flex ${sizeClass} shrink-0 items-center justify-center rounded-full bg-primary/10 font-semibold text-primary`}
    >
      {userInitials(name)}
    </span>
  );
}

export function YouBadge() {
  const t = useTranslations("usersPage");
  return (
    <span className="inline-flex shrink-0 items-center rounded-md border border-primary/40 bg-primary/10 px-1.5 py-0.5 text-[10px] font-semibold text-primary">
      {t("youBadge")}
    </span>
  );
}

const INVITE_STATUS_TONES: Record<InviteStatus, string> = {
  active: "border-success/40 bg-success/10 text-success",
  expired: "border-border bg-muted text-muted-foreground",
  revoked: "border-destructive/40 bg-destructive/10 text-destructive",
  exhausted: "border-border bg-muted text-muted-foreground",
};

export function InviteStatusChip({ status }: { status: InviteStatus }) {
  const t = useTranslations("usersPage");
  return (
    <span
      className={`inline-flex items-center rounded-md border px-2 py-0.5 text-xs font-medium ${INVITE_STATUS_TONES[status]}`}
    >
      {t(`inviteStatus.${status}`)}
    </span>
  );
}

export function useNowMs(intervalMs = 60_000) {
  const [nowMs, setNowMs] = useState(() => Date.now());

  useEffect(() => {
    const interval = setInterval(() => setNowMs(Date.now()), intervalMs);
    return () => clearInterval(interval);
  }, [intervalMs]);

  return nowMs;
}
