"use client";

import { useState } from "react";
import { useTranslations } from "next-intl";
import {
  Building2,
  ChevronsUpDown,
  LogIn,
  PlusCircle,
} from "lucide-react";

import { Avatar, AvatarFallback } from "@/components/ui/avatar";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";

type TenantAccessSwitcherProps = {
  createAction: (formData: FormData) => void | Promise<void>;
  identityEmail: string;
};

export function TenantAccessSwitcher({
  createAction,
  identityEmail,
}: TenantAccessSwitcherProps) {
  const t = useTranslations("onboarding");
  const [open, setOpen] = useState(false);
  const [showCreateDialog, setShowCreateDialog] = useState(false);

  return (
    <>
      <div className="grid gap-3 px-4 py-4">
        <Popover open={open} onOpenChange={setOpen}>
          <PopoverTrigger
            render={
              <Button
                aria-expanded={open}
                aria-haspopup="dialog"
                aria-label={t("accessActionLabel")}
                className="h-11 w-full justify-between rounded-md px-3 sm:w-[300px]"
                variant="outline"
              />
            }
          >
            <span className="flex min-w-0 items-center gap-2">
              <Avatar size="sm">
                <AvatarFallback className="bg-accent text-xs font-semibold text-accent-foreground">
                  <Building2 className="size-3.5" />
                </AvatarFallback>
              </Avatar>
              <span className="truncate text-left">
                <span className="block text-sm font-medium">
                  {t("accessAction")}
                </span>
                <span className="block truncate text-xs text-muted-foreground">
                  {identityEmail}
                </span>
              </span>
            </span>
            <ChevronsUpDown className="size-4 shrink-0 text-muted-foreground" />
          </PopoverTrigger>
          <PopoverContent align="start" className="w-[300px] p-0" sideOffset={6}>
            <div className="py-1">
              <div className="px-2 py-1.5 text-xs font-medium text-muted-foreground">
                {t("accessGroup")}
              </div>
              <TenantAccessMenuItem
                description={t("createMessage")}
                icon={<PlusCircle className="size-4" />}
                label={t("createTitle")}
                onSelect={() => {
                  setOpen(false);
                  setShowCreateDialog(true);
                }}
              />
              <TenantAccessMenuLink
                description={t("joinMessage")}
                href="/join"
                icon={<LogIn className="size-4" />}
                label={t("joinTitle")}
              />
            </div>
          </PopoverContent>
        </Popover>
        <p className="max-w-xl text-sm text-muted-foreground">
          {t("accessActionDescription")}
        </p>
      </div>

      <Dialog open={showCreateDialog} onOpenChange={setShowCreateDialog}>
        <DialogContent closeLabel={t("cancel")} className="sm:max-w-[425px]">
          <form action={createAction} className="grid gap-4">
            <DialogHeader>
              <DialogTitle>{t("createTitle")}</DialogTitle>
              <DialogDescription>{t("createMessage")}</DialogDescription>
            </DialogHeader>
            <div className="grid gap-3">
              <div className="grid gap-1.5">
                <Label htmlFor="display_name">{t("tenantName")}</Label>
                <Input id="display_name" name="display_name" required />
              </div>
              <div className="grid gap-1.5">
                <Label htmlFor="slug">{t("tenantSlug")}</Label>
                <Input id="slug" name="slug" required />
              </div>
            </div>
            <DialogFooter className="-mx-4 -mb-4">
              <Button
                onClick={() => setShowCreateDialog(false)}
                type="button"
                variant="outline"
              >
                {t("cancel")}
              </Button>
              <Button type="submit">
                <PlusCircle className="size-4" />
                {t("createSubmit")}
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </>
  );
}

function TenantAccessMenuItem({
  description,
  icon,
  label,
  onSelect,
}: {
  description: string;
  icon: React.ReactNode;
  label: string;
  onSelect: () => void;
}) {
  return (
    <Button
      className="h-auto w-full justify-start gap-2 rounded-none px-2 py-2 text-left font-normal"
      onClick={onSelect}
      type="button"
      variant="ghost"
    >
      <span className="flex size-7 shrink-0 items-center justify-center rounded-md border border-border bg-background text-muted-foreground">
        {icon}
      </span>
      <span className="min-w-0 flex-1">
        <span className="block font-medium text-foreground">{label}</span>
        <span className="block truncate text-xs text-muted-foreground">
          {description}
        </span>
      </span>
    </Button>
  );
}

function TenantAccessMenuLink({
  description,
  href,
  icon,
  label,
}: {
  description: string;
  href: string;
  icon: React.ReactNode;
  label: string;
}) {
  return (
    <a
      className="flex w-full items-center gap-2 px-2 py-2 text-left text-sm transition-colors duration-150 ease-out hover:bg-muted focus-visible:bg-muted"
      href={href}
    >
      <span className="flex size-7 shrink-0 items-center justify-center rounded-md border border-border bg-background text-muted-foreground">
        {icon}
      </span>
      <span className="min-w-0 flex-1">
        <span className="block font-medium text-foreground">{label}</span>
        <span className="block truncate text-xs text-muted-foreground">
          {description}
        </span>
      </span>
    </a>
  );
}
