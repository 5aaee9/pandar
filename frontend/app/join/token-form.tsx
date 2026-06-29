"use client";

import { useEffect, useState } from "react";
import { useTranslations } from "next-intl";
import { LogIn } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";

export function JoinTokenForm({
  action,
}: {
  action: (formData: FormData) => void;
}) {
  const t = useTranslations("join");
  const [token, setToken] = useState("");

  useEffect(() => {
    setToken(window.location.hash.slice(1));
  }, []);

  return (
    <form action={action} className="grid gap-4 px-4 py-4">
      <div className="grid gap-1.5">
        <Label htmlFor="join-token">{t("joinToken")}</Label>
        <Input
          className="font-mono"
          id="join-token"
          name="token"
          onChange={(event) => setToken(event.target.value)}
          required
          value={token}
        />
      </div>
      <Button
        className="justify-center bg-cyan-700 text-white hover:bg-cyan-800"
        type="submit"
      >
        <LogIn className="size-4" />
        {t("joinSubmit")}
      </Button>
    </form>
  );
}
