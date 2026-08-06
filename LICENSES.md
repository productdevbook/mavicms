# Licences

Mavi CMS is MIT. See [LICENSE](LICENSE).

That choice constrains what can be brought into it, and this file is the
record of having checked rather than assumed.

## What could not be borrowed

[listmonk](https://github.com/knadh/listmonk) is a good newsletter and mailing
program, and an obvious place to look when writing anything that sends mail.
It is **AGPL-3.0**.

No code from it is here, and none can be. The AGPL is not an attribution
licence: copying from it would make this project AGPL too, which would bind
every agency and every site owner running it — including their own
modifications, over a network, to anyone who uses the service. That is not a
decision an attribution line can carry, and it is not one to make on somebody
else's behalf by pasting a function.

Reading it to learn what a mail integration needs is fine — an API's required
fields are facts, and facts are not copyrightable. Implementations are.
`backend/api/src/email.rs` is written against Amazon's published API.

## What the mail plugin uses

Amazon's own SDK, `aws-sdk-sesv2`, under **Apache-2.0** — permissive, and
carried without difficulty by an MIT project. Signature Version 4 is four
HMACs in an order nothing tells you is wrong; there is no reason for this
project to be the one maintaining that.

## Dependencies

Checked with `cargo license` and `license-checker-rseidelsohn`.

**Rust — 479 packages.** Every one permissive: Apache-2.0 OR MIT (296), MIT
(92), Apache-2.0 (27), Unicode-3.0 (18), and smaller numbers of ISC,
BSD-3-Clause and Zlib. Nothing under the GPL, the AGPL, the SSPL or a source-
available licence. `r-efi` offers LGPL-2.1 as one of three options; MIT is
another, and MIT is the one taken.

**JavaScript — 534 packages.** MIT (476), ISC (21), BSD (15), Apache-2.0 (6),
MPL-2.0 (6), and single packages under 0BSD, Unlicense, BlueOak-1.0.0 and
Python-2.0. Again nothing copyleft in a way that reaches this code.

Four are worth naming, because a tool that only counts licences will flag them
and the answer should be written down once:

- **`@tiptap/extension-table-of-contents`** reports `SEE LICENSE IN
  LICENSE.md`, which counters report as "Custom". It is MIT. Tiptap's
  table-of-contents extension was a paid one before version 3; this is not
  that version.
- **`lightningcss`** and its platform binaries are **MPL-2.0**. Weak copyleft,
  per file: using it as a build tool obliges nothing here, and it does not
  ship in the built site.
- **`caniuse-lite`** is **CC-BY-4.0** — a browser-support dataset used while
  building, requiring attribution, which this is.
- **Inter** (`@fontsource-variable/inter`) is **OFL-1.1**. The font ships to
  readers, so its licence ships with it: it is in
  `node_modules/@fontsource-variable/inter/LICENSE` and is included in the
  build output.

## Re-checking

Neither list is fixed. Before adding a dependency:

```sh
cd backend && cargo license           # every Rust crate and its licence
bunx license-checker-rseidelsohn --production --summary
```

A GPL, AGPL, SSPL or source-available dependency is not a thing to weigh
against convenience — this project is MIT, and staying MIT is the promise
made to whoever forked it.
