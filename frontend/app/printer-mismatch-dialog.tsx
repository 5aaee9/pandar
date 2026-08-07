"use client";

import {
  createContext,
  useActionState,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { useTranslations } from "next-intl";
import { XIcon } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import type { Printer } from "./dashboard-types";
import {
  availablePlateMismatchActions,
  plateRecoveryIssue,
  type PlateMismatchAction,
  type PlateRecoveryIssueKind,
} from "./plate-mismatch-actions";
import {
  handlePrintError,
  type PrinterRecoveryActionState,
} from "./printer-recovery-actions";

type MismatchContextValue = {
  open: (occurrenceKey: string) => void;
};

const MismatchContext = createContext<MismatchContextValue | null>(null);
const IDLE_ACTION_STATE: PrinterRecoveryActionState = { status: "idle" };

export function PrinterMismatchCoordinator({
  printers,
  children,
}: {
  printers: Printer[];
  children: ReactNode;
}) {
  const occurrences = useMemo(
    () =>
      printers.flatMap((printer) =>
        printerPlateRecoveryIssue(printer)
          ? [{ key: mismatchOccurrenceKey(printer), printer }]
          : [],
      ),
    [printers],
  );
  const occurrenceKeys = occurrences.map(({ key }) => key).join("\u0000");
  const opened = useRef(new Set<string>());
  const selectedKeyRef = useRef<string | null>(null);
  const [selectedKey, setSelectedKey] = useState<string | null>(null);

  useEffect(() => {
    const current = selectedKeyRef.current;
    if (current && occurrences.some(({ key }) => key === current)) {
      return;
    }
    const next = occurrences.find(({ key }) => !opened.current.has(key)) ?? null;
    if (next) {
      opened.current.add(next.key);
    }
    selectedKeyRef.current = next?.key ?? null;
    setSelectedKey(selectedKeyRef.current);
  }, [occurrenceKeys, occurrences]);

  const dismiss = useCallback(
    (key: string) => {
      opened.current.add(key);
      const next =
        occurrences.find(
          (occurrence) =>
            occurrence.key !== key && !opened.current.has(occurrence.key),
        ) ?? null;
      if (next) {
        opened.current.add(next.key);
      }
      selectedKeyRef.current = next?.key ?? null;
      setSelectedKey(selectedKeyRef.current);
    },
    [occurrences],
  );
  const context = useMemo<MismatchContextValue>(
    () => ({
      open: (key) => {
        if (occurrences.some((occurrence) => occurrence.key === key)) {
          opened.current.add(key);
          selectedKeyRef.current = key;
          setSelectedKey(key);
        }
      },
    }),
    [occurrences],
  );
  const selected = occurrences.find(({ key }) => key === selectedKey) ?? null;

  return (
    <MismatchContext.Provider value={context}>
      {children}
      {selected ? (
        <PrinterMismatchDialog
          key={selected.key}
          open
          printer={selected.printer}
          onDismiss={() => dismiss(selected.key)}
          onSent={() => dismiss(selected.key)}
        />
      ) : null}
    </MismatchContext.Provider>
  );
}

export function PrinterMismatchWarning({ printer }: { printer: Printer }) {
  const t = useTranslations("printMonitor");
  const coordinator = useContext(MismatchContext);
  const issue = printerPlateRecoveryIssue(printer);
  if (!issue) {
    return null;
  }
  const copy = plateRecoveryCopy(issue.kind);
  const key = mismatchOccurrenceKey(printer);
  return (
    <div className="mt-3 rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">
      <div className="font-medium">{t(copy.title)}</div>
      <Button
        aria-label={t(copy.review, { name: printer.name })}
        className="mt-1 h-auto p-0 text-xs text-inherit underline underline-offset-2"
        onClick={() => coordinator?.open(key)}
        type="button"
        variant="link"
      >
        {t("reviewAction")}
      </Button>
    </div>
  );
}

function PrinterMismatchDialog({
  printer,
  open,
  onDismiss,
  onSent,
}: {
  printer: Printer;
  open: boolean;
  onDismiss: () => void;
  onSent: () => void;
}) {
  const t = useTranslations("printMonitor");
  const [state, formAction, pending] = useActionState(
    handlePrintError,
    IDLE_ACTION_STATE,
  );
  const actions = availablePlateMismatchActions(printer);
  const issue = printerPlateRecoveryIssue(printer);

  useEffect(() => {
    if (state.status === "sent") {
      onSent();
    }
  }, [onSent, state.status]);

  if (!issue) {
    return null;
  }
  const copy = plateRecoveryCopy(issue.kind);

  return (
    <Dialog
      open={open}
      onOpenChange={(nextOpen) => {
        if (!nextOpen && !pending) {
          onDismiss();
        }
      }}
    >
      <DialogContent className="sm:max-w-2xl" showCloseButton={false}>
        <Button
          aria-label={t(copy.close)}
          className="absolute right-2 top-2"
          disabled={pending}
          onClick={onDismiss}
          size="icon-sm"
          type="button"
          variant="ghost"
        >
          <XIcon />
        </Button>
        <DialogHeader>
          <DialogTitle>{t("warningTitle")}</DialogTitle>
          <DialogDescription>{printer.name}</DialogDescription>
        </DialogHeader>
        <div className="rounded-md bg-muted px-3 py-2 font-mono text-sm font-medium">
          {issue.code}
        </div>
        <p className="text-sm text-muted-foreground">{t(copy.explanation)}</p>
        {state.status === "error" ? (
          <p className="text-sm text-destructive" role="alert">
            {state.error}
          </p>
        ) : null}
        <form action={formAction}>
          <input name="tenant_id" type="hidden" value={printer.tenant_id} />
          <input name="printer_id" type="hidden" value={printer.id} />
          <input
            name="error_generation"
            type="hidden"
            value={printer.print?.error_generation ?? 0}
          />
          {actions.length === 0 ? (
            <p className="text-sm text-muted-foreground">{t("printerOnly")}</p>
          ) : (
            <DialogFooter className="mt-4 flex-col sm:flex-row">
              {actions.map((action) => (
                <Button
                  disabled={pending}
                  key={action}
                  name="error_action"
                  type="submit"
                  value={action}
                  variant={action === "stop" ? "destructive" : "outline"}
                >
                  {actionLabel(action, t)}
                </Button>
              ))}
            </DialogFooter>
          )}
        </form>
      </DialogContent>
    </Dialog>
  );
}

function actionLabel(
  action: PlateMismatchAction,
  t: ReturnType<typeof useTranslations<"printMonitor">>,
): string {
  switch (action) {
    case "resume":
      return t("resumeAction");
    case "ignore":
      return t("ignoreAction");
    case "stop":
      return t("stopAction");
  }
}

function printerPlateRecoveryIssue(printer: Printer) {
  return plateRecoveryIssue(printer.serial_number, printer.print?.print_error);
}

const PLATE_RECOVERY_COPY = {
  mismatch: {
    close: "closeMismatch",
    explanation: "mismatchExplanation",
    review: "reviewMismatch",
    title: "mismatchTitle",
  },
  missing: {
    close: "closePlateRecovery",
    explanation: "plateMissingExplanation",
    review: "reviewPlateRecovery",
    title: "plateMissingTitle",
  },
  "marker-not-detected": {
    close: "closePlateMarker",
    explanation: "plateMarkerExplanation",
    review: "reviewPlateMarker",
    title: "plateMarkerTitle",
  },
  misaligned: {
    close: "closePlateRecovery",
    explanation: "plateOffsetExplanation",
    review: "reviewPlateRecovery",
    title: "plateOffsetTitle",
  },
  "misaligned-with-debris": {
    close: "closePlateRecovery",
    explanation: "plateOffsetDebrisExplanation",
    review: "reviewPlateRecovery",
    title: "plateOffsetTitle",
  },
  "collision-risk": {
    close: "closePlateRecovery",
    explanation: "plateCollisionExplanation",
    review: "reviewPlateRecovery",
    title: "plateCollisionTitle",
  },
  "encoder-board-missing": {
    close: "closePlateRecovery",
    explanation: "encoderBoardMissingExplanation",
    review: "reviewPlateRecovery",
    title: "encoderBoardMissingTitle",
  },
} as const;

function plateRecoveryCopy(kind: PlateRecoveryIssueKind) {
  return PLATE_RECOVERY_COPY[kind];
}

function mismatchOccurrenceKey(printer: Printer): string {
  return `${printer.id}:${printer.print?.error_generation ?? 0}`;
}
