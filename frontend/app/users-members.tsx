"use client";

import { useMemo, useState } from "react";
import { useTranslations } from "next-intl";
import { ChevronRightIcon, SearchIcon } from "lucide-react";

import { useAdminDate } from "./admin-model";
import type { Tenant, User, UserIdentity } from "./dashboard-types";
import { EmptyState, SectionHeader, Tag } from "./dashboard-ui";
import {
  inputClasses,
  rowHoverClasses,
  tableScrollClasses,
} from "../lib/utils";
import { Button } from "@/components/ui/button";
import { MemberDialog } from "./users-member-dialog";
import {
  countByRole,
  filterUsers,
  isSelf,
  sortUsers,
  type RoleFilter,
} from "./users-model";
import { UserAvatar, YouBadge } from "./users-shared";

const ROLE_FILTERS: Exclude<RoleFilter, "all">[] = [
  "tenant_admin",
  "operator",
  "viewer",
];

export function MembersSection({
  tenant,
  users,
  identities,
  meEmail,
}: {
  tenant: Tenant;
  users: User[];
  identities: UserIdentity[];
  meEmail: string | null;
}) {
  const t = useTranslations("usersPage");
  const tTokens = useTranslations("tokens");
  const formatDate = useAdminDate();
  const [query, setQuery] = useState("");
  const [roleFilter, setRoleFilter] = useState<RoleFilter>("all");
  const [selectedUserId, setSelectedUserId] = useState<string | null>(null);

  const sorted = useMemo(() => sortUsers(users), [users]);
  const counts = useMemo(() => countByRole(users), [users]);
  const visible = useMemo(
    () => filterUsers(sorted, query, roleFilter),
    [sorted, query, roleFilter],
  );
  const identitiesByUser = useMemo(() => {
    const map = new Map<string, UserIdentity[]>();
    for (const identity of identities) {
      const current = map.get(identity.user_id) ?? [];
      current.push(identity);
      map.set(identity.user_id, current);
    }
    return map;
  }, [identities]);
  const selectedUser = selectedUserId
    ? (sorted.find((user) => user.id === selectedUserId) ?? null)
    : null;

  const filterChip = (id: RoleFilter, label: string, count: number) => {
    const active = roleFilter === id;
    return (
      <Button
        aria-pressed={active}
        className={`h-auto rounded-md px-2 py-1 text-xs ${
          active ? "border-primary/40" : "text-muted-foreground hover:text-muted-foreground"
        }`}
        key={id}
        onClick={() => setRoleFilter(id)}
        type="button"
        variant={active ? "soft" : "outline"}
      >
        {label} · {count}
      </Button>
    );
  };

  return (
    <section className="overflow-hidden rounded-md border border-border bg-card">
      <SectionHeader
        title={t("membersTitle")}
        subtitle={t("membersSubtitle", { name: tenant.display_name })}
        meta={t("membersMeta", { count: users.length })}
      />
      {users.length === 0 ? (
        <EmptyState title={t("noMembersTitle")} message={t("noMembersMessage")} />
      ) : (
        <>
          <div className="flex flex-col gap-3 border-b border-border px-4 py-3 sm:flex-row sm:items-center sm:justify-between">
            <div className="relative sm:w-64">
              <SearchIcon
                aria-hidden="true"
                className="absolute left-2.5 top-1/2 size-4 -translate-y-1/2 text-muted-foreground"
              />
              <input
                aria-label={t("searchLabel")}
                className={`${inputClasses} pl-8`}
                onChange={(event) => setQuery(event.target.value)}
                placeholder={t("searchPlaceholder")}
                type="search"
                value={query}
              />
            </div>
            <div
              aria-label={t("filterLabel")}
              className="flex flex-wrap items-center gap-1.5"
              role="group"
            >
              {filterChip("all", t("filterAll"), users.length)}
              {ROLE_FILTERS.map((role) =>
                filterChip(
                  role,
                  tTokens.has(role) ? tTokens(role) : role,
                  counts[role],
                ),
              )}
            </div>
          </div>
          {visible.length === 0 ? (
            <EmptyState
              title={t("noResultsTitle")}
              message={t("noResultsMessage")}
            />
          ) : (
            <div className={tableScrollClasses}>
              <table className="min-w-full text-left text-sm">
                <thead className="bg-muted/60 text-xs font-semibold text-muted-foreground">
                  <tr>
                    <th className="px-4 py-2.5">{t("colMember")}</th>
                    <th className="px-4 py-2.5">{t("colRole")}</th>
                    <th className="px-4 py-2.5">{t("colIdentities")}</th>
                    <th className="px-4 py-2.5">{t("colJoined")}</th>
                    <th className="px-4 py-2.5">
                      <span className="sr-only">{t("manageMember")}</span>
                    </th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-border">
                  {visible.map((user) => {
                    const linked = identitiesByUser.get(user.id) ?? [];
                    return (
                      <tr key={user.id} className={rowHoverClasses}>
                        <td className="px-4 py-3">
                          <Button
                            className="h-auto w-full min-w-0 justify-start gap-3 rounded-md p-0 font-normal hover:bg-transparent dark:hover:bg-transparent"
                            onClick={() => setSelectedUserId(user.id)}
                            type="button"
                            variant="ghost"
                          >
                            <UserAvatar name={user.display_name} />
                            <span className="min-w-0">
                              <span className="flex items-center gap-2">
                                <span className="truncate font-medium text-foreground">
                                  {user.display_name}
                                </span>
                                {isSelf(user, meEmail) ? <YouBadge /> : null}
                              </span>
                              <span className="block truncate text-xs text-muted-foreground">
                                {user.email}
                              </span>
                            </span>
                          </Button>
                        </td>
                        <td className="px-4 py-3">
                          <Tag value={user.role} />
                        </td>
                        <td className="px-4 py-3">
                          {linked.length === 0 ? (
                            <span className="text-xs text-muted-foreground">
                              -
                            </span>
                          ) : (
                            <span className="flex flex-wrap gap-1">
                              {linked.map((identity) => (
                                <Tag key={identity.id} value={identity.provider} />
                              ))}
                            </span>
                          )}
                        </td>
                        <td className="px-4 py-3 text-xs text-muted-foreground">
                          {formatDate(user.created_at)}
                        </td>
                        <td className="px-4 py-3 text-right">
                          <Button
                            aria-label={t("manageFor", {
                              user: user.display_name,
                            })}
                            className="text-muted-foreground hover:text-foreground"
                            onClick={() => setSelectedUserId(user.id)}
                            size="icon-sm"
                            type="button"
                            variant="ghost"
                          >
                            <ChevronRightIcon
                              aria-hidden="true"
                              className="size-4"
                            />
                          </Button>
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          )}
        </>
      )}
      {selectedUser ? (
        <MemberDialog
          identities={identitiesByUser.get(selectedUser.id) ?? []}
          meEmail={meEmail}
          onOpenChange={(open) => {
            if (!open) {
              setSelectedUserId(null);
            }
          }}
          open
          tenant={tenant}
          user={selectedUser}
          users={sorted}
        />
      ) : null}
    </section>
  );
}
