import dayjs from "dayjs";
import relativeTime from "dayjs/plugin/relativeTime";
import "dayjs/locale/zh-cn";

dayjs.extend(relativeTime);

export type RelativeTime = {
  relative: string;
  timestampMs: number;
};

export function getRelativeTime(
  value: string,
  nowMs: number,
  locale: string,
): RelativeTime | null {
  const timestamp = dayjs(value);
  if (nowMs === 0 || !timestamp.isValid()) {
    return null;
  }

  return {
    relative: timestamp
      .locale(locale.toLowerCase().startsWith("zh") ? "zh-cn" : "en")
      .from(dayjs(nowMs)),
    timestampMs: timestamp.valueOf(),
  };
}
