/*
 * What a listing of a Claude Code marketplace's plugins printed, read back:
 * `mindweaver gather marketplace --list --json` prints its catalog in the
 * envelope every reading command shares, whose `data` is core's
 * `marketplace::Listing`, written into TypeScript as `MarketplaceListing`,
 * as the TUI reads the same line back into the same type. The JSON rather
 * than the table, since a table's columns are for reading, and the
 * envelope's names are a contract a test of the CLI holds.
 */
import type { Envelope } from '@/api/generated/Envelope'
import type { MarketplaceListing } from '@/api/generated/MarketplaceListing'
import type { Warning } from '@/api/generated/Warning'
import type { Ran } from '@/api/transport'

/** A marketplace's catalog as a listing printed it, and what it said beside it. */
export interface Listed {
  listing: MarketplaceListing
  /** Each line it said on stderr: a plugin whose entry could not be read, a `headersHelper` never run. */
  said: Warning[]
}

/**
 * What the last line of `ran` printed, read as a listing's envelope: none
 * when it could not run, or printed no envelope holding a catalog, which
 * the run's own toast and drawer then say.
 */
export function listed(ran: Ran): Listed | undefined {
  const run = ran.outcome.runs.at(-1)
  if (!run || run.error !== null) {
    return undefined
  }
  try {
    const envelope = JSON.parse(run.stdout) as Envelope<MarketplaceListing>
    return envelope.data?.catalog ? { listing: envelope.data, said: run.stderr } : undefined
  } catch {
    return undefined
  }
}
