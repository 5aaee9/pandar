export function mixedAmsLiteGlobalTrayId(
  unitKind: string | null | undefined,
  trayId: number,
): number | null {
  return unitKind === "ams_lite_mixed" && trayId >= 0 && trayId < 4
    ? 24 + trayId
    : null;
}
