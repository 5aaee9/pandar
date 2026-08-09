import { mkdir, readFile, readdir, writeFile } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const sourceDirectory = path.join(root, 'reference/BambuStudio/resources/hms')
const outputDirectory = path.join(root, 'frontend/app/hms-catalog-data')

await mkdir(outputDirectory, { recursive: true })

for (const locale of ['en', 'zh-cn']) {
  const prefix = `hms_${locale}_`
  const files = (await readdir(sourceDirectory))
    .filter((file) => file.startsWith(prefix) && file.endsWith('.json'))
    .sort()
  const prefixes = files.map((file) => file.slice(prefix.length, -'.json'.length).toUpperCase())
  const messages = []
  const messageIndexes = new Map()
  const codes = {}

  for (const [prefixIndex, file] of files.entries()) {
    const source = JSON.parse(await readFile(path.join(sourceDirectory, file), 'utf8'))
    const seenCodes = new Set()
    for (const item of source.data.device_hms[locale]) {
      if (!Object.hasOwn(item, 'intro')) {
        continue
      }

      const code = item.ecode.toUpperCase()
      if (seenCodes.has(code)) {
        continue
      }
      seenCodes.add(code)

      if (!item.intro) {
        continue
      }

      let messageIndex = messageIndexes.get(item.intro)
      if (messageIndex === undefined) {
        messageIndex = messages.length
        messages.push(item.intro)
        messageIndexes.set(item.intro, messageIndex)
      }

      const entries = codes[code] ??= Array(prefixes.length).fill(null)
      entries[prefixIndex] = messageIndex
    }
  }

  const sortedCodes = Object.fromEntries(
    Object.entries(codes).sort(([left], [right]) => left.localeCompare(right)),
  )
  await writeFile(
    path.join(outputDirectory, `${locale}.json`),
    `${JSON.stringify({ prefixes, messages, codes: sortedCodes })}\n`,
  )
}
