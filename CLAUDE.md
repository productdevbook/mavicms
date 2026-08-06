# Working on Mavi CMS

This repository is public. Everything in it — code, comments, tests, commit
messages, documentation — is readable by anyone, forever, including by search
engines and by whoever forks it.

## Nothing about whoever is running it

This is the rule that matters most, and the one that has already been broken:
a commit message and a doc comment carried two real customer email addresses
into public history.

Never write, in code or in a commit message:

- **Email addresses, names, or company names** of anyone using this software.
- **Hostnames** of anybody's installation.
- **Anything from a database** somebody is actually using — post titles,
  categories, user names.
- **Credentials of any kind**, including ones that have since been changed. A
  rotated password still tells an attacker how you choose passwords.
- **Server addresses, cluster details, or bucket names** belonging to an
  operator.

Something that happened while running an instance is often the reason a fix
exists, and that reason is worth writing down. Write down the *shape* of it:

> An agency whose address matches an editor already on the site would have
> taken that account over.

not

> The agency is a@example.com and so is the editor.

The same goes for test data. Use names that are obviously invented.

If you find any of this already in the repository, say so plainly, take it out
of the working tree, and rewrite the history that carries it — the commit
message is as public as the code.

## Everything else

- `cargo fmt`, `cargo clippy --all-targets -- -D warnings` and `cargo test`
  before every backend commit. CI checks the formatter, and it is the one that
  gets forgotten.
- `bun run build && bun run typecheck && bun run lint` before every frontend
  commit. `tsc --noEmit` checks nothing here; the build is what generates the
  route tree.
- `bun run extract` after touching any user-facing string, and translate the
  new ones — the panel ships English and Turkish and a half-translated screen
  is worse than an untranslated one.
- Comments explain what the code cannot: a constraint that reads as wrong, an
  external behaviour nobody would guess, the reason a choice was made. Not
  what the line does, not what changed, not a changelog.
- Commit messages say why, in prose. What changed is in the diff.
