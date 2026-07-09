export function formatDate(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }

  return date.toLocaleString("en-US", {
    dateStyle: "medium",
    timeStyle: "short",
    timeZone: "UTC",
  });
}

export function formatBytes(
  value: number,
  formatNumber?: (n: number) => string,
) {
  const fmt = (n: number) => (formatNumber ? formatNumber(n) : n.toFixed(1));
  if (value < 1024) {
    return `${formatNumber ? formatNumber(value) : value} B`;
  }
  if (value < 1024 * 1024) {
    return `${fmt(value / 1024)} KiB`;
  }

  return `${fmt(value / (1024 * 1024))} MiB`;
}
