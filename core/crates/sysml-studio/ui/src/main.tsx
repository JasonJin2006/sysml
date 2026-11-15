import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
// Tailwind first: its preflight is the base layer, and Radix Themes plus the hand-written rules
// below are meant to override it rather than the other way round.
import './styles.generated.css';
import '@radix-ui/themes/styles.css';
import './styles.css';
import { Theme } from '@radix-ui/themes';
import App from './App';
import { DesignKit } from './design/DesignKit';
import { IdeSkeleton } from './design/IdeSkeleton';
import { applyHostPalette, hostAppearance, installHostBridge, isEmbedded, onHost } from './host';
import { adoptTheme, applyTheme, useTheme } from './theme';

// Embedded, the editor owns the palette: a canvas in one colour scheme inside an editor in another
// reads as a broken panel, and the person already chose their theme there. The host names that
// choice in the fragment, and a fragment is readable before the first paint — so the page is drawn
// in the right palette rather than drawn in its own and corrected a frame later.
const hosted = hostAppearance();
if (hosted) adoptTheme(hosted);
else applyTheme();

// The colours themselves still arrive as a message, because they are a table of fifty values and a
// fragment is no place for one. `theme` says light or dark, which is what Radix and Monaco switch
// on; `palette` carries the editor's actual colours, which is what makes the picture and its
// toolbar look like the window around them rather than like a different tool inside a frame.
installHostBridge();
if (isEmbedded()) {
  // The stylesheet needs to know as much as the component tree does: this one attribute is what
  // lets the wrapper Radix Themes adds take the height of the panel instead of its content's.
  document.documentElement.dataset.sysmlEmbedded = '';
  onHost({
    theme: (appearance) => adoptTheme(appearance),
    palette: (colors) => applyHostPalette(colors),
  });
}

function Root() {
  const [appearance] = useTheme();
  return (
    <Theme accentColor="blue" grayColor="slate" radius="small" scaling="95%" appearance={appearance}>
      <App />
    </Theme>
  );
}

const host = document.getElementById('root');
if (!host) throw new Error('index.html has no #root');

// `#design` renders the design system's own kitchen sink instead of the studio: one page holding
// every token and component in both palettes, which is the only sane way to review the language
// without judging it through a half-finished screen. `#ide` renders the same language applied to a
// full IDE shell, which is where the question "does this style survive contact with a canvas and
// a code surface" actually gets answered.
const route = window.location.hash;
// `#ide-light` exists so the light palette can be captured and reviewed without clicking the
// toggle first; the shell has no business knowing about review shortcuts beyond this.
if (route === '#ide-light') document.documentElement.dataset.theme = 'light';

createRoot(host).render(
  <StrictMode>
    {route === '#design' ? (
      <DesignKit />
    ) : route === '#ide' || route === '#ide-light' ? (
      <IdeSkeleton />
    ) : (
      <Root />
    )}
  </StrictMode>,
);
