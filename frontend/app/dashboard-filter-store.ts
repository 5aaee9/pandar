import { create } from "zustand";

type DashboardFilterState = {
  query: string;
  status: string;
  setQuery: (query: string) => void;
  setStatus: (status: string) => void;
  reset: () => void;
};

export const useDashboardFilterStore = create<DashboardFilterState>((set) => ({
  query: "",
  status: "all",
  setQuery: (query) => set({ query }),
  setStatus: (status) => set({ status }),
  reset: () => set({ query: "", status: "all" }),
}));
