import nodemailer from "nodemailer";

import type { env } from "./env.ts";

type EmailConfig = typeof env.email;

type MagicLinkEmail = {
  config: EmailConfig;
  to: string;
  url: string;
  ttlSeconds: number;
};

export function magicLinkSubject(brandName: string): string {
  return `Sign in to ${brandName}`;
}

export function magicLinkText(
  brandName: string,
  url: string,
  ttlSeconds: number,
): string {
  return [
    `Use this link to sign in to ${brandName}:`,
    "",
    url,
    "",
    `This link expires in ${formatDuration(ttlSeconds)}.`,
    "If you did not request this email, you can ignore it.",
  ].join("\n");
}

export function magicLinkHtml(
  brandName: string,
  url: string,
  ttlSeconds: number,
): string {
  const escapedBrandName = escapeHtml(brandName);
  const escapedUrl = escapeHtml(url);

  return [
    "<!doctype html>",
    '<html lang="en">',
    "<body>",
    `<p>Use this link to sign in to ${escapedBrandName}:</p>`,
    `<p><a href="${escapedUrl}">${escapedUrl}</a></p>`,
    `<p>This link expires in ${escapeHtml(formatDuration(ttlSeconds))}.</p>`,
    "<p>If you did not request this email, you can ignore it.</p>",
    "</body>",
    "</html>",
  ].join("");
}

export async function sendMagicLinkEmail(email: MagicLinkEmail): Promise<void> {
  if (email.config.provider === "resend") {
    await sendResendEmail(email);
    return;
  }

  await sendSmtpEmail(email);
}

function formatDuration(ttlSeconds: number): string {
  if (ttlSeconds % 3_600 === 0) {
    const hours = ttlSeconds / 3_600;
    return `${hours} ${hours === 1 ? "hour" : "hours"}`;
  }

  if (ttlSeconds % 60 === 0) {
    const minutes = ttlSeconds / 60;
    return `${minutes} ${minutes === 1 ? "minute" : "minutes"}`;
  }

  return `${ttlSeconds} ${ttlSeconds === 1 ? "second" : "seconds"}`;
}

function escapeHtml(value: string): string {
  const entities: Record<string, string> = {
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;",
  };

  return value.replace(/[&<>"']/g, (character) => entities[character]);
}

async function sendResendEmail(email: MagicLinkEmail): Promise<void> {
  const config = email.config;
  if (config.provider !== "resend") {
    throw new Error("Resend email config is required");
  }

  const response = await fetch("https://api.resend.com/emails", {
    method: "POST",
    headers: {
      Authorization: `Bearer ${config.apiKey}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify({
      from: config.from,
      to: email.to,
      subject: magicLinkSubject(config.brandName),
      text: magicLinkText(config.brandName, email.url, email.ttlSeconds),
      html: magicLinkHtml(config.brandName, email.url, email.ttlSeconds),
    }),
  });

  if (!response.ok) {
    const body = await response.text();
    throw new Error(`Resend email failed with ${response.status}: ${body}`);
  }
}

async function sendSmtpEmail(email: MagicLinkEmail): Promise<void> {
  const config = email.config;
  if (config.provider !== "smtp") {
    throw new Error("SMTP email config is required");
  }

  const transport = nodemailer.createTransport({
    host: config.host,
    port: config.port,
    secure: config.tls === "tls",
    requireTLS: config.tls === "starttls",
    ignoreTLS: config.tls === "none",
    auth: {
      user: config.username,
      pass: config.password,
    },
  });

  await transport.sendMail({
    from: config.from,
    to: email.to,
    subject: magicLinkSubject(config.brandName),
    text: magicLinkText(config.brandName, email.url, email.ttlSeconds),
    html: magicLinkHtml(config.brandName, email.url, email.ttlSeconds),
  });
}
