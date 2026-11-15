/**
 * The project's files, grouped for the navigator.
 *
 * A model is a set of files, and the paths arrive as strings from the server. Showing them as a
 * flat list of full paths is unreadable past three files, so they are grouped by directory and
 * the directory the files share is dropped: a project whose files all sit in `core/examples/`
 * should not repeat that prefix on every row.
 */

export interface FileGroup {
  /** The directory, relative to the shared root. Empty for files at the root itself. */
  dir: string;
  files: FileRow[];
}

export interface FileRow {
  /** The full path, which is what the server addresses the file by. */
  path: string;
  /** The file's own name, without its directory. */
  name: string;
}

/** The directory part of a path, in either separator style. */
function directoryOf(path: string): string {
  const cut = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
  return cut < 0 ? '' : path.slice(0, cut);
}

/** The file's own name. */
export function nameOf(path: string): string {
  const cut = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
  return cut < 0 ? path : path.slice(cut + 1);
}

/**
 * The longest directory prefix every file shares, as a segment count.
 *
 * Compared by segments, not characters: `/a/bc` and `/a/bd` share `/a/b` as text but `/a` as a
 * directory, and trimming by characters would produce a half segment.
 */
function sharedSegments(directories: string[]): string[] {
  if (directories.length === 0) return [];
  const split = (dir: string) => dir.split(/[\\/]/).filter((part) => part !== '');
  const first = split(directories[0]);
  let common = first.length;
  for (const dir of directories.slice(1)) {
    const parts = split(dir);
    let i = 0;
    while (i < common && i < parts.length && parts[i] === first[i]) i += 1;
    common = i;
  }
  return first.slice(0, common);
}

/**
 * Group the files by directory, with the shared prefix removed and the groups in path order.
 *
 * A single file is its own group with an empty directory: there is nothing to group it with, and
 * a heading repeating its own name would be noise.
 */
export function groupFiles(paths: readonly string[]): FileGroup[] {
  if (paths.length === 0) return [];
  const shared = sharedSegments(paths.map(directoryOf));
  const byDir = new Map<string, FileRow[]>();
  for (const path of paths) {
    const dir = directoryOf(path);
    const parts = dir.split(/[\\/]/).filter((part) => part !== '');
    const rest = parts.slice(shared.length).join('/');
    const rows = byDir.get(rest) ?? [];
    rows.push({ path, name: nameOf(path) });
    byDir.set(rest, rows);
  }
  return [...byDir.entries()]
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([dir, files]) => ({
      dir,
      files: [...files].sort((a, b) => a.name.localeCompare(b.name)),
    }));
}
