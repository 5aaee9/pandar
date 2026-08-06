"use client";

import { useEffect, useState } from "react";

// Offset below the viewport top where a section counts as current. Roughly
// the sticky dashboard header plus a small breathing room.
const MARKER_PX = 128;

export function useScrollSpy(sectionIds: readonly string[]): string {
  const [activeId, setActiveId] = useState(sectionIds[0] ?? "");

  useEffect(() => {
    let frame = 0;

    const update = () => {
      frame = 0;
      let next = sectionIds[0] ?? "";
      for (const id of sectionIds) {
        const element = document.getElementById(id);
        if (element && element.getBoundingClientRect().top <= MARKER_PX) {
          next = id;
        }
      }
      const lastId = sectionIds[sectionIds.length - 1];
      const last = lastId ? document.getElementById(lastId) : null;
      if (last && last.getBoundingClientRect().top <= window.innerHeight * 0.6) {
        next = lastId;
      }
      setActiveId(next);
    };

    const onScroll = () => {
      if (frame === 0) {
        frame = requestAnimationFrame(update);
      }
    };

    update();
    window.addEventListener("scroll", onScroll, { passive: true });
    window.addEventListener("resize", onScroll);
    return () => {
      window.removeEventListener("scroll", onScroll);
      window.removeEventListener("resize", onScroll);
      if (frame !== 0) {
        cancelAnimationFrame(frame);
      }
    };
  }, [sectionIds]);

  return activeId;
}
