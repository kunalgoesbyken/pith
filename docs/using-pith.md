# Using Pith

The tray shows what Pith is doing (Ready, Paused until 14:35, Skipped: password field, Loading model). It pauses Pith for 15 minutes, an hour or until you resume, and turns it off in the app you were just using. Check for updates in the same menu asks GitHub once for the latest release and shows the result there; the release page opens in your browser only when you click it. In a code editor, which Pith skips by default, the same item reads Allow Pith in that app.

Settings changes apply at once:

- Enabled, fix typos, and how often suggestions appear (Fewer, Normal, More)
- Suggestion length (Short, Normal, Long)
- The model (Small, Standard or Large, with the recommended one marked). Choosing one downloads it if needed and switches without a restart; downloaded models are kept
- Ghost text opacity
- Your stats: suggestions seen and accepted today and in total, and the time they saved you
- The apps with their own setting: where Pith is turned off, and the code editors where you turned it on
- Start with Windows (off by default)
- Open data folder, Delete model and data

Settings also warns when something else suggests in the same fields: Windows text suggestions for hardware keyboards (with a button to turn them off), PhraseExpress or Lightkey.

When an app reports no caret, or a caret Pith cannot trust, the suggestion shows in a small pill just below the field, or inside its bottom edge when there is no room. The pill keeps its own background and contrast, so the opacity setting does not apply to it.

The pill is also used when the only text after the caret on the line is closing punctuation, such as `)`, `]`, `"`, `.`, `?`, `」` or `）。` (closing brackets and quotes, and terminal punctuation in any script, by Unicode property). Pith suggests the words that belong before it and shows them in a pill that starts at the caret, just below its line, so the suggestion never sits on top of the punctuation. In a field with several lines the pill covers the start of the line under the caret while it is shown. The pill spans from the caret to the right edge of the field (or of the screen when the app does not report a field) and wraps up to four lines. When the suggestion does not fit, Pith cuts it to the longest run of whole words that does and still fits the rules above, and accepting inserts only those; when not even one word fits on the first row, nothing is shown, the tray says "Skipped: no room for the suggestion", and Pith tries once more when the window or the caret moves and again whenever you edit the text. Resizing a field without moving the caret does not count as a move unless the pill is anchored to the field itself, so Pith usually waits until you type. This applies to every pill and to the ghost text, so what you accept is always what you saw. A suggestion that repeats the punctuation after the caret is dropped (`could you send it` before `?`, not `could you send it?`), and so is one that ends in a comma, semicolon or colon (or the equivalent in another script), ends a sentence in front of punctuation that already ends one (`soon.` before `).`), closes a bracket it did not open or runs over more than one sentence (an abbreviation such as `Dr. Smith` counts as a sentence break, so suggestions containing one are dropped). Accepting inserts at the caret and leaves the punctuation where it is. Any other text after the caret on the line (a word, an opening bracket, a symbol) still skips the suggestion.

Pith pauses and frees the model while Windows battery saver is on.

## Typo fixes

When you type a space after a word the Windows spell checker does not know, Pith shows the correction in a pill under the caret instead of a completion, for example `teh → the`, with the fix in stronger text. Tab (or Shift+Tab or Ctrl+Right, or any key you set in `accept_keys` or `accept_word_keys`) replaces the word and keeps the space and the caret where they were. Esc dismisses it, and so does typing on without taking it: either way that word is not offered again until you restart Pith. The completion for that spot waits while the check runs (the first check of a session can take a moment while Windows loads its dictionaries; Pith waits at most one second) and while the fix is shown, then carries on once you accept, dismiss or type. In manual trigger mode no fix is offered unprompted.

- It works in every language Windows has a spell checker for. Pith asks for the language of your current keyboard layout (`en-US`, `de-DE`, `hi-IN`) and falls back to the language without its region. With no checker for that language, nothing is offered. The language is the layout's, not the text's: words of another language typed on the wrong layout are checked against the layout's dictionary, and the one-edit rule is the only guard against a wrong fix. Words you added to the Windows dictionary count as correct, and your Windows autocorrect list is honoured.
- Only the word just before the caret and the space you typed are checked, and only when the checker finds exactly that word wrong and the best suggestion is one edit away (a swapped, missing, extra or wrong letter) or comes from your autocorrect list. A fix is always a single word of at most 32 letters, so autocorrect entries that expand into several words are ignored. Anything less certain is left alone.
- It never touches a password field, text after the caret, a word under three letters, a word with digits, an ALL-CAPS word or one with capitals inside it (`NASA`, `iPhone`), a capitalised word in the middle of a sentence (names), or a word that is part of a URL, an email address, a path, a hyphenated word or an identifier. Blocked apps, pauses and the per-app switch in the tray apply here as they do to completions.
- Applying a fix selects the word through UI Automation, checks that the selected text is exactly the word you typed, and pastes the correction over it, so it is one undo step. If the app cannot select the word, or the selection does not read back as that word, nothing is replaced and that word is not offered again until you restart Pith. If focus moves to another field or the field cannot be read before the selection happens, nothing is replaced and the word can be offered again. If the field changes while the selection is being made, nothing is pasted, the word is not declined, and the caret is put back if you have not changed the selection yourself. Until the word is selected (at most three seconds), Pith starts no completion or spell check; the paste itself then lands as a normal edit.
- When you type the space while a completion is already showing and the word matches its start, Pith keeps typing through that completion and does not check the word.
- The Fix typos toggle in Settings, or `typo_fix = false`, turns it off.

## Nearby text

Pith adds a few lines from around the box you type in to the start of the prompt, so a reply knows what it replies to: the box's own label, the subject and recipient fields above it, the headings and captions near it and the last message of the conversation above it. Lines are the app's own words in any language, with no labels added by Pith.

- Windows walks the UI Automation tree upwards from the field (at most 6 levels, 12 siblings each, about 60 ms) and Linux walks the AT-SPI tree the same way. The result is cached per field and window title, so typing costs nothing extra, and a new walk happens when the field or the title changes or when the box is cleared after a send.
- Password boxes, hidden and off-screen items and lines that are mostly symbols or numbers are skipped, and so is anything the title or your own last 200 characters already say.
- The lines go before the text you typed, so the model's cache is reused while you type. They are capped at `nearby_tokens` (300) and by what is left of `context_tokens` after the text you typed. The first suggestion in a field takes a little longer because the lines are read by the model once.
- With `remote_url` set nothing from around the box is sent, only the text you typed as before. The Use nearby text switch in Settings, Privacy, or `nearby_context = false`, turns the whole feature off.

## Your style

When you turn on Learn my writing style in Settings, Privacy & data, Pith remembers the finished sentences in which you kept a suggestion with Tab, so later suggestions sound more like you. Nothing is learned until you do.

- A sentence is learned only after you accept part of a suggestion and then finish that sentence with a full stop, a question or exclamation mark, or a new line. Only the sentence you finished is kept, between 12 and 160 characters of ordinary prose. Sentences with an email address, a link, a long number or a code-like word are dropped.
- Up to three of the most recent sentences from the same app and the same script (Latin, Devanagari, Han, Arabic and so on) go in front of the text you typed, within 96 tokens. This works in any language and needs no model change. A small model can repeat an example word for word.
- Sentences carry over between conversations in the same app. They are never used in another app. Turning Pith off in an app forgets what it learned there, and nothing is learned while Pith is paused.
- With `remote_url` set nothing is sent: the examples are left out, so the server sees only the text you typed as before.
- Settings lists every remembered sentence with the app it came from. Forget removes one, Forget all removes the file. Turning the switch off stops learning and stops using the sentences, and they stay in the list until you remove them.
- On Windows the file is encrypted with DPAPI, so another Windows account or a copied disk cannot read it, but a program running as you can. On Linux there is no encryption: `~/.local/share/pith/style.toml` is a plain file that only your account can read (mode 0600 in a 0700 folder), and you remove sentences by editing or deleting it while Pith is closed.
