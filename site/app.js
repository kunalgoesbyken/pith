const samples = [
  { typed: 'Thank you for getting back to me so quickly. I have read the proposal and', ghost: ' will send my notes by Friday.' },
  { typed: 'Pushed the fix to staging, can someone', ghost: ' take a look before we merge?' },
  { typed: 'Vielen Dank für Ihre schnelle Antwort. Ich melde mich', ghost: ' morgen mit den Unterlagen.' },
];

const tryBox = document.querySelector('[data-try]');

if (tryBox) {
  const field = tryBox.querySelector('[role="textbox"]');
  const typedEl = tryBox.querySelector('[data-typed]');
  const ghostEl = tryBox.querySelector('[data-ghost]');
  const live = document.createElement('p');
  live.className = 'visually-hidden';
  live.setAttribute('aria-live', 'polite');
  tryBox.append(live);

  const state = { index: 0, typed: samples[0].typed, ghost: samples[0].ghost, shown: true };
  let timer = 0;

  const render = (fresh) => {
    typedEl.textContent = state.typed;
    ghostEl.textContent = state.shown ? state.ghost : '';
    ghostEl.classList.toggle('is-new', Boolean(fresh));
    live.textContent = fresh && state.ghost ? `Suggestion: ${state.ghost.trim()}` : '';
  };

  const later = (ms, fn) => {
    clearTimeout(timer);
    timer = setTimeout(fn, ms);
  };

  const next = () => {
    state.index = (state.index + 1) % samples.length;
    state.typed = samples[state.index].typed;
    state.ghost = samples[state.index].ghost;
    state.shown = false;
    render(false);
    later(700, () => {
      state.shown = true;
      render(true);
    });
  };

  const keep = (part) => {
    if (!state.shown || !state.ghost) return false;
    const taken = part === 'word' ? state.ghost.match(/^\s*\S+/)[0] : state.ghost;
    state.typed += taken;
    state.ghost = state.ghost.slice(taken.length);
    render(false);
    if (!state.ghost) later(1400, next);
    return true;
  };

  const hide = () => {
    if (!state.shown) return false;
    state.shown = false;
    render(false);
    later(1800, next);
    return true;
  };

  const type = (char) => {
    state.typed += char;
    if (state.shown && state.ghost.startsWith(char)) {
      state.ghost = state.ghost.slice(char.length);
      render(false);
      if (!state.ghost) later(1400, next);
      return;
    }
    state.ghost = '';
    state.shown = false;
    render(false);
    later(1800, next);
  };

  field.addEventListener('keydown', (event) => {
    const plain = !event.ctrlKey && !event.metaKey && !event.altKey;
    let handled = false;
    if (event.key === 'Tab' && !event.shiftKey) handled = keep('all');
    else if ((event.key === 'Tab' && event.shiftKey) || (event.key === 'ArrowRight' && event.ctrlKey)) handled = keep('word');
    else if (event.key === 'Escape') handled = hide();
    else if (event.key === 'Backspace') {
      state.typed = state.typed.slice(0, -1);
      state.shown = false;
      render(false);
      later(1800, next);
      handled = true;
    } else if (event.key.length === 1 && plain) {
      type(event.key);
      handled = true;
    }
    if (handled) event.preventDefault();
  });

  tryBox.querySelectorAll('[data-act]').forEach((button) => {
    button.addEventListener('click', () => {
      const act = button.dataset.act;
      if (act === 'hide') hide();
      else keep(act);
      field.focus({ preventScroll: true });
    });
  });
}

document.querySelectorAll('[data-switch]').forEach((root) => {
  const tabs = [...root.querySelectorAll('[role="tab"]')];
  const panels = tabs.map((tab) => document.getElementById(tab.getAttribute('aria-controls')));

  const select = (index, focus) => {
    tabs.forEach((tab, i) => {
      const on = i === index;
      tab.setAttribute('aria-selected', String(on));
      tab.tabIndex = on ? 0 : -1;
      panels[i].hidden = !on;
    });
    if (focus) tabs[index].focus();
  };

  tabs.forEach((tab, i) => {
    tab.addEventListener('click', () => select(i, false));
    tab.addEventListener('keydown', (event) => {
      const step = { ArrowRight: 1, ArrowLeft: -1 }[event.key];
      if (event.key === 'Home') select(0, true);
      else if (event.key === 'End') select(tabs.length - 1, true);
      else if (step) select((i + step + tabs.length) % tabs.length, true);
      else return;
      event.preventDefault();
    });
  });

  select(0, false);
});
