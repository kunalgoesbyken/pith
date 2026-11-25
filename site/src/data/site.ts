const repo = 'https://github.com/kunalgoesbyken/pith';

export const links = {
  repo,
  download: `${repo}/releases/latest/download/pith.exe`,
  latest: `${repo}/releases/latest`,
  releases: `${repo}/releases`,
  privacy: `${repo}/blob/master/docs/privacy.md`,
  licence: `${repo}/blob/master/LICENSE`,
};

export const nav = [
  { href: '#how', label: 'How it works' },
  { href: '#samples', label: 'Languages' },
  { href: '#privacy', label: 'Privacy' },
  { href: '#models', label: 'Models' },
];

export const demo = [
  { typed: 'Thanks for the quick reply. I read the proposal and', ghost: ' will send notes by Friday.' },
  { typed: 'Pushed the fix to staging, can someone', ghost: ' take a look before we merge?' },
  { typed: 'Vielen Dank für Ihre schnelle Antwort. Ich melde mich', ghost: ' morgen mit den Unterlagen.', lang: 'de' },
];

export const ticker = [
  { typed: 'Could you send', ghost: ' the numbers?' },
  { typed: 'Ich melde mich', ghost: ' morgen.', lang: 'de' },
  { typed: 'Let me know if', ghost: ' Thursday works.' },
  { typed: 'Te escribo para', ghost: ' confirmar.', lang: 'es' },
];

export const samples = [
  { name: 'Work email', typed: 'Hi Sam, thanks for the update. Could you send me', ghost: ' the final numbers before Thursday?', saved: 23, tint: 'butter' },
  { name: 'Long document', typed: 'The second phase of the project focuses on', ghost: ' reducing the time it takes to onboard a new team.', saved: 34, tint: 'peach' },
  { name: 'Slack', typed: 'Pushed the fix to staging, can someone', ghost: ' take a look before we merge?', saved: 21, tint: 'sky' },
  { name: 'German', typed: 'Ich wollte nur kurz nachfragen, ob', ghost: ' Sie die Unterlagen schon erhalten haben.', saved: 44, tint: 'sage', lang: 'de' },
  { name: 'Spanish', typed: 'Gracias por tu mensaje. Te escribo para', ghost: ' confirmar la reunión del jueves.', saved: 40, tint: 'paper', lang: 'es' },
];

export const models = [
  { name: 'Small', note: 'English first', download: '271 MB', memory: '370 MB', keystroke: '148 ms', saved: '26%' },
  { name: 'Standard', note: 'Many languages', download: '397 MB', memory: '590 MB', keystroke: '206 ms', saved: '29%', pick: 'Most PCs' },
  { name: 'Large', note: 'Best suggestions', download: '1,107 MB', memory: '1,270 MB', keystroke: '520 ms', saved: '37%' },
];

export const privacy = [
  { term: 'What you type', detail: 'Kept in memory, never saved or sent.' },
  { term: 'Network', detail: 'One model download, and updates you ask for.' },
  { term: 'Passwords', detail: 'Password fields and managers are never read.' },
  { term: 'Telemetry', detail: 'None.' },
];

export const faq = [
  { q: 'Is it free?', a: 'Yes. GPL-3.0 licence, no account, no word limit.' },
  { q: 'Why does Windows warn me?', a: 'The exe is not signed yet. Check it against SHA256SUMS on the release page.' },
  { q: 'Which apps?', a: 'Chrome, Edge, Electron apps and Notepad, tested every week.' },
  { q: 'Which languages?', a: 'English is best. German and Spanish work well.' },
];
