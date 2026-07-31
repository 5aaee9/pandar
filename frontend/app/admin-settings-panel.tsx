import { useActionState, useState } from "react";
import { useTranslations } from "next-intl";
import { UserPlusIcon } from "lucide-react";

import { FormattedDate } from "../components/formatted-date";
import { createAgentPairing } from "./admin-actions";
import { rowHoverClasses } from "../lib/utils";
import {
  Input,
  PrimaryButton,
  SecretActionResult,
  Subhead,
} from "./admin-panel-shared";
import { TenantTokensTable } from "./admin-settings-token-list";
import type { Agent, AuditEvent, Tenant, TenantToken } from "./dashboard-types";
import { DetailLine, StatusBadge } from "./dashboard-ui";

export function CreateAgentPairingForm({ tenantId }: { tenantId: string }) {
  const [nonce, setNonce] = useState(0);
  return (
    <CreateAgentPairingFormInner
      key={nonce}
      onCreateAnother={() => setNonce((value) => value + 1)}
      tenantId={tenantId}
    />
  );
}

function CreateAgentPairingFormInner({
  tenantId,
  onCreateAnother,
}: {
  tenantId: string;
  onCreateAnother: () => void;
}) {
  const t = useTranslations("admin");
  const [state, formAction, pending] = useActionState(createAgentPairing, null);
  const locked = pending || state?.ok === true;

  return (
    <form action={formAction} className="grid gap-3">
      <input name="tenant_id" type="hidden" value={tenantId} />
      <div className="text-sm font-semibold text-foreground">
        {t("pairAgent")}
      </div>
      <Input name="name" label={t("agentName")} required disabled={locked} />
      {locked ? null : (
        <PrimaryButton label={pending ? t("creating") : t("createPairing")} />
      )}
      <SecretActionResult state={state} />
      {state?.ok ? (
        <button
          className="inline-flex items-center gap-1 self-start text-xs font-medium text-primary underline-offset-4 transition-colors duration-150 ease-out hover:text-primary/80 hover:underline"
          onClick={onCreateAnother}
          type="button"
        >
          <UserPlusIcon aria-hidden="true" className="size-3" />
          {t("createAnother")}
        </button>
      ) : null}
    </form>
  );
}

export function TenantSecretsPanel({
  selectedTenant,
  tenantTokens,
  agents,
  nowMs,
}: {
  selectedTenant: Tenant;
  tenantTokens?: TenantToken[];
  agents?: Agent[];
  nowMs: number;
}) {
  return (
    <>
      {tenantTokens ? (
        <TenantTokensTable
          tenantId={selectedTenant.id}
          tokens={tenantTokens}
          nowMs={nowMs}
        />
      ) : null}
      {agents ? <AgentsList agents={agents} /> : null}
    </>
  );
}

export function TenantAuditPanel({
  auditEvents,
}: {
  selectedTenant: Tenant;
  auditEvents: AuditEvent[];
}) {
  return <AuditList events={auditEvents} />;
}

function AgentsList({ agents }: { agents: Agent[] }) {
  const t = useTranslations("admin");
  return (
    <div>
      <Subhead
        title={t("agents")}
        meta={t("agentsMeta", { count: agents.length })}
      />
      <div className="grid gap-2 px-4 py-3">
        {agents.length === 0 ? (
          <div className="text-sm text-muted-foreground">
            {t("noLinkedAgents")}
          </div>
        ) : (
          agents.map((agent) => (
            <div
              key={agent.id}
              className={`rounded-md border border-border bg-muted/20 px-3 py-2 text-sm ${rowHoverClasses}`}
            >
              <div className="flex items-center justify-between gap-2">
                <span className="font-medium text-foreground">
                  {agent.name}
                </span>
                <StatusBadge value={agent.status} />
              </div>
              <DetailLine label={t("idLabel")} value={agent.id} mono />
            </div>
          ))
        )}
      </div>
    </div>
  );
}

function AuditList({ events }: { events: AuditEvent[] }) {
  const t = useTranslations("admin");
  return (
    <div>
      <Subhead
        title={t("auditEvents")}
        meta={t("auditMeta", { count: events.length })}
      />
      <div className="divide-y divide-border">
        {events.length === 0 ? (
          <div className="px-4 py-3 text-sm text-muted-foreground">
            {t("noAuditEvents")}
          </div>
        ) : (
          events.map((event) => (
            <div key={event.id} className="px-4 py-3 text-sm">
              <div className="font-medium text-foreground">{event.action}</div>
              <div className="mt-1 text-xs text-muted-foreground">
                {event.actor_type} · {event.target_type} ·{" "}
                <FormattedDate value={event.created_at} />
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
