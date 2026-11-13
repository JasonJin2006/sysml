import { useEffect, useMemo, useRef, useState } from 'react';
import { t } from './i18n';

/**
 * The command palette.
 *
 * Every control the toolbar has is also a command, reachable by `Ctrl+Shift+P` and filtered by
 * typing. It exists because the toolbar cannot hold everything: a tool with thirty buttons in a
 * row is a tool whose user reads a row of thirty buttons.
 *
 * The matching is a subsequence, not a substring: `gv` finds "general view" and `chk` finds
 * "check the model", which is what a person means by typing the first letters of the words.
 */
import { Command, filterCommands } from './commands';

export type { Command };

export function CommandPalette({
  open,
  commands,
  onClose,
}: {
  open: boolean;
  commands: Command[];
  onClose: () => void;
}) {
  const [query, setQuery] = useState('');
  const [active, setActive] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  const visible = useMemo(() => filterCommands(commands, query, (c) => t(c.id)), [commands, query]);

  useEffect(() => {
    if (!open) return;
    setQuery('');
    setActive(0);
    // Focus after the element exists: the palette is rendered by this same change.
    window.setTimeout(() => input.current?.focus(), 0);
  }, [open]);

  if (!open) return null;

  return (
    <div className="palette-backdrop" onMouseDown={onClose}>
      <div
        className="palette"
        role="dialog"
        aria-label={t('Command palette')}
        onMouseDown={(event) => event.stopPropagation()}
      >
        <input
          ref={input}
          className="palette-input"
          value={query}
          placeholder={t('Type a command')}
          onChange={(event) => {
            setQuery(event.target.value);
            setActive(0);
          }}
          onKeyDown={(event) => {
            if (event.key === 'Escape') {
              event.preventDefault();
              onClose();
              return;
            }
            if (event.key === 'ArrowDown') {
              event.preventDefault();
              setActive((at) => Math.min(at + 1, visible.length - 1));
              return;
            }
            if (event.key === 'ArrowUp') {
              event.preventDefault();
              setActive((at) => Math.max(at - 1, 0));
              return;
            }
            if (event.key === 'Enter') {
              event.preventDefault();
              const chosen = visible[active];
              if (chosen && !chosen.disabled) {
                onClose();
                chosen.run();
              }
            }
          }}
        />
        <ul className="palette-list" role="listbox">
          {visible.length === 0 && <li className="palette-empty">{t('No command matches')}</li>}
          {visible.map((command, index) => (
            <li
              key={command.id}
              role="option"
              aria-selected={index === active}
              className={`palette-item${index === active ? ' on' : ''}${command.disabled ? ' off' : ''}`}
              title={command.disabled ? command.reason : undefined}
              onMouseEnter={() => setActive(index)}
              onMouseDown={(event) => {
                event.preventDefault();
                if (command.disabled) return;
                onClose();
                command.run();
              }}
            >
              <span className="palette-title">{t(command.id)}</span>
              {command.keys && <span className="palette-keys">{command.keys}</span>}
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}
