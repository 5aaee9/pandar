import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { redirect } from "next/navigation";
import { Component, type ReactNode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { QueryErrorBoundary } from "./query-error-boundary";

class NavigationBoundary extends Component<
  { children: ReactNode },
  { navigationCaught: boolean }
> {
  state = { navigationCaught: false };

  static getDerivedStateFromError() {
    return { navigationCaught: true };
  }

  render() {
    return this.state.navigationCaught ? <p>Navigation handled</p> : this.props.children;
  }
}

function ThrowingContent(): ReactNode {
  throw new Error("query failed");
}

describe("QueryErrorBoundary", () => {
  afterEach(() => vi.restoreAllMocks());

  it("lets Next.js handle redirects thrown by a server form action", async () => {
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => {});
    const queryClient = new QueryClient();
    const user = userEvent.setup();

    render(
      <QueryClientProvider client={queryClient}>
        <NavigationBoundary>
          <QueryErrorBoundary>
            <form
              action={async () => {
                redirect("/devices");
              }}
            >
              <button type="submit">Pause print</button>
            </form>
          </QueryErrorBoundary>
        </NavigationBoundary>
      </QueryClientProvider>,
    );

    await user.click(screen.getByRole("button", { name: "Pause print" }));

    await waitFor(() => expect(screen.getByText("Navigation handled")).toBeVisible());
    expect(screen.queryByText("Failed to load data")).not.toBeInTheDocument();
    expect(consoleError).not.toHaveBeenCalledWith("Query error:", expect.anything());
  });

  it("still handles ordinary descendant errors", () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    const queryClient = new QueryClient();

    render(
      <QueryClientProvider client={queryClient}>
        <QueryErrorBoundary>
          <ThrowingContent />
        </QueryErrorBoundary>
      </QueryClientProvider>,
    );

    expect(screen.getByText("Failed to load data")).toBeVisible();
  });
});
