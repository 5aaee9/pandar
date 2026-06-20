const apiUrl = process.env.APP_API_URL ?? 'http://localhost:8080'

export default function Page() {
  return (
    <main className="min-h-screen bg-zinc-950 px-6 py-10 text-zinc-100">
      <section className="mx-auto flex max-w-5xl flex-col gap-8">
        <div>
          <p className="text-sm font-medium uppercase tracking-wide text-emerald-300">Pandar</p>
          <h1 className="mt-3 text-4xl font-semibold">Bambu Studio cloud alternative</h1>
          <p className="mt-4 max-w-2xl text-base leading-7 text-zinc-300">
            Manage tenants, users, agents, and Bambu printer links through a self-hosted hub.
          </p>
        </div>

        <div className="grid gap-4 md:grid-cols-3">
          <StatusCard label="Hub API" value={apiUrl} />
          <StatusCard label="Agent channel" value="gRPC reverse connection" />
          <StatusCard label="Machine channel" value="SFTP and MQTT" />
        </div>
      </section>
    </main>
  )
}

function StatusCard({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-md border border-zinc-800 bg-zinc-900 p-4">
      <div className="text-sm text-zinc-400">{label}</div>
      <div className="mt-2 break-words text-lg font-medium">{value}</div>
    </div>
  )
}
