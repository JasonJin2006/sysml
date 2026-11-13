import { useEffect, useRef } from 'react';
import * as monaco from 'monaco-editor/editor/editor.api';
import 'monaco-editor/basic-languages/monaco.contribution';
import 'monaco-editor/editor/contrib/find/browser/findController';
import 'monaco-editor/editor/contrib/suggest/browser/suggestController';
import { t } from './i18n';

/**
 * The source editor.
 *
 * Monaco, the editor behind VS Code, rather than a textarea: bracket matching, folding, search and
 * multi-cursor are what make a 2000-line model workable, and none of them is worth writing.
 *
 * The language is registered as `sysml` with no tokenizer of its own — the keywords come from the
 * standard library's spelling and the structural cues are handled by bracket matching. A real
 * tokenizer belongs in the language server, where one implementation serves every editor.
 *
 * Longest line matters: the editor is created with `automaticLayout`, so it follows the splitter
 * without the caller watching a resize event.
 */
const LANGUAGE = 'sysml';

let registered = false;

/** Register the language once, on first use. */
function register(): void {
  if (registered) return;
  registered = true;
  monaco.languages.register({ id: LANGUAGE, extensions: ['.sysml', '.kerml'] });
  monaco.languages.setLanguageConfiguration(LANGUAGE, {
    comments: { lineComment: '//', blockComment: ['/*', '*/'] },
    brackets: [
      ['{', '}'],
      ['[', ']'],
      ['(', ')'],
    ],
    autoClosingPairs: [
      { open: '{', close: '}' },
      { open: '[', close: ']' },
      { open: '(', close: ')' },
      { open: '"', close: '"' },
      { open: "'", close: "'" },
      { open: '/*', close: '*/' },
    ],
    surroundingPairs: [
      { open: '{', close: '}' },
      { open: '[', close: ']' },
      { open: '(', close: ')' },
      { open: '"', close: '"' },
      { open: "'", close: "'" },
    ],
    folding: {
      markers: { start: /^\s*\/\/\s*#?region\b/, end: /^\s*\/\/\s*#?endregion\b/ },
    },
  });
  monaco.languages.setMonarchTokensProvider(LANGUAGE, {
    // Enough to tell a comment from a string from a keyword. Anything that needs the tree — "is
    // this name a definition or a use" — is the language server's job, not a regex's.
    keywords: [
      'package', 'library', 'namespace', 'import', 'private', 'public', 'protected',
      'part', 'port', 'item', 'attribute', 'action', 'state', 'requirement', 'constraint',
      'calc', 'case', 'analysis', 'verification', 'use', 'view', 'viewpoint', 'rendering',
      'metadata', 'enum', 'interface', 'connection', 'allocation', 'flow', 'perform',
      'exhibit', 'expose', 'satisfy', 'verify', 'concern', 'subject', 'objective',
      'actor', 'stakeholder', 'doc', 'comment', 'rep', 'derived', 'abstract', 'variation',
      'variant', 'ref', 'individual', 'snapshot', 'timeslice', 'occurrence', 'def',
      'in', 'out', 'inout', 'require', 'assume', 'assert', 'then', 'else', 'if', 'while',
      'for', 'loop', 'fork', 'join', 'merge', 'decide', 'send', 'accept', 'return',
    ],
    typeKeywords: ['Boolean', 'String', 'Integer', 'Real', 'Natural'],
    operators: ['::', ':>', ':>>', '=>', '->', '..', '==', '!=', '<=', '>=', '**', '?'],
    symbols: /[=><!~?:&|+\-*/^%]+/,
    tokenizer: {
      root: [
        [/\/\/.*$/, 'comment'],
        [/\/\*/, 'comment', '@block'],
        [/'(?:[^'\\]|\\.)*'/, 'string'],
        [/"(?:[^"\\]|\\.)*"/, 'string'],
        [/[A-Za-z_]\w*/, { cases: { '@keywords': 'keyword', '@typeKeywords': 'type', '@default': 'identifier' } }],
        [/\d+(\.\d+)?/, 'number'],
        [/[{}()[\]]/, '@brackets'],
        [/[<>](?!@symbols)/, '@brackets'],
        [/@symbols/, { cases: { '@operators': 'operator', '@default': '' } }],
      ],
      block: [
        [/[^*/]+/, 'comment'],
        [/\*\//, 'comment', '@pop'],
        [/[*/]/, 'comment'],
      ],
    },
  });
}

export interface EditorProps {
  /** The file's path, as the document listed it. */
  path: string | null;
  /** The text to show. `null` while it loads. */
  value: string | null;
  /** Called with the whole text after a pause in typing, and again on save. */
  onChange: (text: string) => void;
  /** Called when the person presses Ctrl+S. */
  onSave: () => void;
  /** Whether the buffer has changes the server has not been told about. */
  dirty: boolean;
  /** A key the page was asked to reveal, so the editor can scroll to it and select the name. */
  reveal?: { line: number; nameStart: number; nameEnd: number } | null;
  /**
   * Called with the 1-based line the cursor moved to.
   *
   * The canvas selects the box declared on that line, so the picture follows the cursor. It fires
   * on selection change rather than on every keystroke: typing inside a line is not a move.
   */
  onCursor?: (line: number) => void;
  /** A save is in flight. */
  saving: boolean;
}

export function Editor({
  path,
  value,
  onChange,
  onSave,
  dirty,
  reveal,
  onCursor,
  saving,
}: EditorProps) {
  const host = useRef<HTMLDivElement>(null);
  const editor = useRef<monaco.editor.IStandaloneCodeEditor | null>(null);
  const change = useRef(onChange);
  change.current = onChange;
  const save = useRef(onSave);
  save.current = onSave;
  const cursor = useRef(onCursor ?? (() => undefined));
  cursor.current = onCursor ?? (() => undefined);
  // Read once by the mount effect, which runs before any later `value` reaches the instance.
  const initialText = useRef(value ?? '');

  useEffect(() => {
    if (!host.current || editor.current) return;
    register();
    // A worker would be a second file to ship, and it buys nothing here: the model already has a
    // resolver and a checker, and those live in the server. Without a worker Monaco falls back to
    // running nothing in the background, which is what a syntax-only editor wants.
    monaco.editor.defineTheme('sysml-light', {
      base: 'vs',
      inherit: true,
      rules: [],
      colors: { 'editorLineNumber.foreground': '#9aa0a6', 'editor.lineHighlightBackground': '#f3f6fa' },
    });
    const instance = monaco.editor.create(host.current, {
      // The first text is written here rather than by the effect below: `create` runs inside this
      // mount effect, and the effect that syncs `value` has already run by then on the first
      // render. It read `editor.current === null`, skipped, and never ran again — the page held
      // the file while the editor showed nothing.
      value: initialText.current,
      language: LANGUAGE,
      theme: 'sysml-light',
      automaticLayout: true,
      fontFamily: "ui-monospace, 'Cascadia Code', 'SF Mono', Consolas, monospace",
      fontSize: 12.5,
      lineHeight: 19,
      minimap: { enabled: false },
      scrollBeyondLastLine: false,
      renderWhitespace: 'selection',
      tabSize: 4,
      insertSpaces: true,
      wordWrap: 'off',
    });
    editor.current = instance;
    instance.onDidChangeModelContent(() => {
      change.current(instance.getValue());
    });
    // The cursor is reported on a *selection* change, which includes a click and an arrow key but
    // not a keystroke in the middle of a line. The page uses it to select the box declared on that
    // line, so it must not fire while the person is typing.
    instance.onDidChangeCursorPosition((event) => {
      if (event.source === 'mouse' || event.source === 'keyboard') {
        cursor.current(event.position.lineNumber);
      }
    });
    instance.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => save.current());
    return () => {
      instance.dispose();
      editor.current = null;
    };
  }, []);

  // The server is the owner of the text. When it sends a new version and the editor has nothing
  // unsaved, replace what is shown; when it has unsaved changes, leave them alone — the person is
  // typing, and overwriting their cursor mid-word is the one thing an editor must not do.
  useEffect(() => {
    const instance = editor.current;
    if (!instance || value === null || dirty) return;
    if (instance.getValue() !== value) instance.setValue(value);
  }, [value, path, dirty]);

  useEffect(() => {
    const instance = editor.current;
    if (!instance || !reveal) return;
    const model = instance.getModel();
    if (!model) return;
    // A zero-length range at byte 0 means "this diagnostic names a line, not a name": scroll there
    // and put the cursor at the start of it, without selecting anything.
    if (reveal.nameStart === 0 && reveal.nameEnd === 0) {
      const line = Math.min(Math.max(1, reveal.line), model.getLineCount());
      instance.setPosition({ lineNumber: line, column: 1 });
      instance.revealLineInCenter(line);
      return;
    }
    const start = model.getPositionAt(reveal.nameStart);
    const end = model.getPositionAt(reveal.nameEnd);
    instance.setSelection(new monaco.Selection(start.lineNumber, start.column, end.lineNumber, end.column));
    instance.revealLineInCenter(start.lineNumber);
  }, [reveal]);

  return (
    <div className="editor">
      <div className="editor-head">
        <span className="editor-path" title={path ?? ''}>
          {path ?? t('No file')}
        </span>
        {dirty && <span className="editor-dirty" title={t('Unsaved changes')}>●</span>}
        {saving && <span className="editor-saving">{t('saving…')}</span>}
        <span className="grow" />
        <button className="plain" disabled={!dirty || saving} onClick={onSave} title={t('Save (Ctrl+S)')}>
          {t('save')}
        </button>
      </div>
      <div className="editor-body" ref={host} />
    </div>
  );
}
