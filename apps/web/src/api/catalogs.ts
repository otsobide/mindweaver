import type { Envelope } from './generated/Envelope'
import type { Warning } from './generated/Warning'
import type { Ran } from './transport'

/*
 * What a search of the catalogs found, as the panel reads it back from the
 * run of its task (`search_skills`, `search_servers`): the line is
 * `mindweaver gather search --json` or `gather mcp search --json`, and what
 * it printed is the envelope every reading command's `--json` prints, its
 * `data` what core found, typed from Rust (the generated `SkillsFound` and
 * `ServerListing`). Read back as the TUI reads the same line's, by the
 * names the envelope gives, never a table's columns.
 *
 * Neither a directory's nor the registry's address reaches the browser:
 * the server asks them, as the line says, and the panel reads only what
 * the line printed. Nothing here is worked out of what was found: a result
 * is shown as core listed it, and sent back whole to be taken.
 */

/** What a search's run said, read back: what it found, and what it said beside it. */
export interface Searched<T> {
  /** What was looked for, as it was sent. */
  query: string
  /** What core found, as the envelope's `data` holds it. */
  found: T
  /**
   * What the line said on stderr, each with its code, a finder skipped for
   * want of a token among it as a hint, which has none.
   */
  said: Warning[]
}

/**
 * What the one line of `ran` printed, read back, for `query`: none when the
 * line did not run, which its error says, or printed what is no envelope,
 * which no line of a search does.
 */
export function searched<T>(query: string, ran: Ran | undefined): Searched<T> | undefined {
  const run = ran?.outcome.runs[0]
  if (!run || run.error !== null) {
    return undefined
  }
  try {
    const envelope = JSON.parse(run.stdout) as Envelope<T>
    return { query, found: envelope.data, said: run.stderr }
  } catch {
    return undefined
  }
}

/**
 * What is sent of what was typed: without the white space around it, which
 * the TUI's search form takes off too, so the same text typed is the same
 * task on either front end, and nothing when that leaves nothing. Unicode's
 * White_Space, which Rust's `str::trim` takes off, rather than JavaScript's
 * `trim`, as the load form does.
 */
export function trimmed(typed: string): string {
  return typed.replace(/^\p{White_Space}+|\p{White_Space}+$/gu, '')
}
