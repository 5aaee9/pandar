import { Skeleton } from "@/components/ui/skeleton";

export default function SettingsLoading() {
  return (
    <div className="mx-auto max-w-5xl">
      <div className="space-y-2 pb-6">
        <Skeleton className="h-8 w-40 rounded-lg" />
        <Skeleton className="h-4 w-72 rounded-lg" />
      </div>
      <div className="grid items-start gap-6 lg:grid-cols-[13rem_minmax(0,1fr)]">
        <Skeleton className="hidden h-44 rounded-xl lg:block" />
        <div className="space-y-6">
          <Skeleton className="h-64 rounded-xl" />
          <Skeleton className="h-40 rounded-xl" />
          <Skeleton className="h-56 rounded-xl" />
        </div>
      </div>
    </div>
  );
}
