import { QueryClient } from "@tanstack/react-query";
import { describe, expect, it, vi } from "vitest";

import {
  invalidateTenantResources,
  mutationResources,
} from "./mutation-invalidation";
import { resourceDataKeys } from "./route-data";

describe("mutation resource ownership", () => {
  it.each(Object.entries(mutationResources))(
    "%s invalidates every declared canonical resource",
    async (_mutation, resources) => {
      const queryClient = new QueryClient();
      const invalidate = vi
        .spyOn(queryClient, "invalidateQueries")
        .mockResolvedValue();

      await invalidateTenantResources(queryClient, "tenant-1", resources);

      expect(
        invalidate.mock.calls.map(([filters]) => filters?.queryKey),
      ).toEqual(
        resources.map((resource) => resourceDataKeys[resource]("tenant-1")),
      );
    },
  );
});
