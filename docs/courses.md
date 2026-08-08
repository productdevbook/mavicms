# Selling courses from Mavi CMS

A plan, not an implementation. It exists because "upload your training videos
and give people access for three months" touches almost every part of this
codebase, and two or three of the decisions are hard to reverse once anybody
is relying on them.

Prices quoted here are from memory and want checking before anybody commits
to a provider.

---

## 1. What is being built

A training company uploads video lessons, arranges them into courses, and lets
people in — some who paid, some who were simply given an account. Access can
run out (three months, a year, never). Lessons can unlock over time rather than
all at once. Students sign in, watch, and pick up where they left off.

That is Kajabi, Teachable, Thinkific. The parts of it that matter:

| | Why it is here |
|---|---|
| Video that plays well on a phone on 4G | The product is video. Everything else is around it. |
| A sign-in that is not the panel's | A student must never be one keystroke from `/api/posts`. |
| Access with an end date | "Three months" is the request. |
| Drip | A course sold as twelve weeks is not twelve weeks if it all arrives on day one. |
| Progress | Somebody coming back after a week needs to know where they were. |
| A member area that exists | The customer wants to sell courses, not to build a member area. |

Payments, coupons, bundles, quizzes and certificates are all real and all
later. §10 says where they go.

---

## 2. The decision everything else hangs off

A membership site has to answer three questions **at the moment of the
request**, not at build time:

1. Is this person signed in?
2. Do they still have access to this course, today?
3. What is the video address, and will it stop working shortly?

A statically built Astro site cannot answer any of them. So: where does the
runtime live?

### It already exists

`backend/api/src/bin/edge.rs` — 422 lines — serves the pages a build published.
It reads the address a request came in on, finds which site that is, and answers
from that site's folder in the bucket. **Every hosted site's hostname already
points at a process of ours that knows which tenant it is talking about and
holds a database connection to that tenant's schema.**

That is the whole of what a member area needs. So:

```
example.com/                     → edge, static, from the bucket   (unchanged)
example.com/ogrenci/…            → edge, the member area           (new)
example.com/api/…                → the API                          (unchanged)
example.com/admin                → the panel                        (unchanged)
```

Adding a site adds no deployment, no container and no configuration file — the
sentence at the top of `edge.rs` — and that stays true. A customer who buys the
course feature gets a member area on their own domain the moment they enable it.

The path is the site's own setting (`/ogrenci`, `/learn`, `/kurslar`), because
it will appear in links people bookmark.

### What we are not doing, and why

- **Astro SSR per site.** Full design control, but every customer's project then
  needs a server and a deploy target that runs one, and `publish` would stop
  meaning "a folder of files". It pushes our problem onto whoever builds the
  front end. Kept as an option for someone who wants it (§11), not the default.
- **A static shell gated in the browser.** Works, costs nothing, and is the
  right answer for the *sales* page — which should be public and indexed anyway.
  Not for the lesson: the notes would be in the static output for anyone with
  the URL.

### The member area's design

It is ours, and that is a real cost — it will not match the customer's site.
Three things make it acceptable, in order of cheapness:

1. It takes the site's title, logo and colours from settings.
2. It accepts a stylesheet the customer supplies.
3. §11: whoever wants to render it themselves gets the API.

Do 1 first. Do not promise 2 until somebody asks.

---

## 3. Where the video goes

**Not through the existing media pipeline.** Worth stating plainly, because it
looks like the obvious place:

```rust
// routes/media.rs
pub const MAX_UPLOAD_BYTES: usize = 10 * 1024 * 1024;   // 10 MB
fn sniff_image(bytes: &[u8]) -> Option<(&'static str, &'static str)>
```

Ten megabytes, raster images only, the whole body buffered in memory, and the
result served publicly from `/uploads`. A one-hour 1080p lesson is one to three
gigabytes. Every one of those four facts is wrong for video, and the last one is
wrong on purpose — an image on a post is meant to be public.

Video needs four things this repo has none of: resumable upload of gigabytes,
transcoding into several bitrates, HLS packaging, and delivery that expires.

### The options

| | Storage | Delivery | Signed URLs | Transcode | Notes |
|---|---|---|---|---|---|
| **Cloudflare Stream** | ~$5 / 1000 min | ~$1 / 1000 min | JWT, first-class | included | Simplest. Upload via tus or a one-time creator URL. Global CDN, no egress surprises. |
| **Bunny Stream** | ~$0.005 /GB/mo | ~$0.005–0.01 /GB | token + expiry | included | Cheapest, and cheap *to Turkey* specifically. Collections API. DRM as an add-on. |
| **Mux** | per minute | per minute | signed playback tokens | included | Best DX and the only real analytics. Priciest by some margin. |
| **R2 + our own ffmpeg** | ~$0.015 /GB/mo | **zero egress** | ours to build | ours to run | Cheapest at scale and by far the most work. See below. |
| Vimeo / YouTube unlisted | — | — | domain lock at best | included | Not access control. Fine for a marketing trailer. |

### Recommendation

**A video plugin, with a provider choice** — exactly the shape `/plugins/s3` and
`/plugins/email` already have, down to the `plugin_setting` row and the
encrypted credentials. Betting the product on one vendor is the mistake here;
the interface is small enough that two providers cost little more than one.

```
trait VideoHost {
    async fn upload_ticket(&self, name: &str) -> Ticket;   // where the browser PUTs
    async fn asset(&self, id: &str) -> Asset;              // duration, status, thumbnail
    async fn playback(&self, id: &str, ttl: Duration, watermark: Option<&str>) -> Playback;
    async fn remove(&self, id: &str);
}
```

Ship **Bunny Stream** and **Cloudflare Stream** first. Which to default to:
Bunny if the customer's audience is Turkish and cost is the deciding factor;
Cloudflare if you would rather not think about it again.

**Self-hosting on R2 is a later chapter, not a first one.** It is genuinely
cheaper — R2 charges nothing for egress, which is the whole bill for video — but
it means a transcode queue, ffmpeg in a container, renditions, and getting HLS
signing right. HLS is a playlist of segments: a presigned GET per segment does
not work through a playlist, so you either sign the playlist and rewrite every
segment URL, or put a Worker in front that checks a token. Both are fine. Both
are a week you do not have to spend in month one.

### Uploading

The browser uploads **straight to the provider**. The CMS never sees the bytes.

```
panel                    CMS                       provider
  │  POST /videos         │                            │
  │──────────────────────>│  ask for an upload ticket  │
  │                       │───────────────────────────>│
  │<──────────────────────│  { url, id }               │
  │                                                    │
  │  tus / PUT, resumable, 2 GB ──────────────────────>│
  │                                                    │
  │                       │<──── webhook: ready ───────│
  │                       │  duration, thumbnail       │
```

Resumable matters: this is a training company's office wifi and a two-gigabyte
file. tus, where the provider offers it.

The webhook is the only inbound address, and it is the one to get right — see
§13.

### Delivering, and making sharing pointless

Playback is minted **per request, after the gate**:

1. Student asks for lesson 4.
2. The API checks: signed in, enrolled, not expired, drip window open.
3. Only then does it ask the provider for a token good for an hour or two.
4. The player gets a URL that is dead by tomorrow.

Plus the student's email address watermarked faintly over the player. It stops
nothing technically and it stops a great deal socially, which is the whole of
what Teachable and Kajabi do. **DRM (Widevine/FairPlay) only when a customer
asks and will pay for it** — it is an add-on with both providers, it breaks on
somebody's browser every month, and the people it stops were never going to buy.

---

## 4. The data model

The rule: reuse what the panel already does well, add tables only where the
shape is genuinely relational.

### A course is a content type

It already is. A site adds a kind called `Eğitim` with `fiyat`, `seviye`,
`süre`; its sales page is a post of that kind; the front end already fetches
posts and already knows how to lay out a kind's fields. Nothing to build.

### A lesson is a post plus a row

A lesson has a title and a body of notes — that is a post, and making it one
buys the whole editor, translations, images and SEO for free. What a post cannot
carry is its place in a course, its video, and when it unlocks, because
`FormField` has no relation type and no video type.

So: a lesson is a post of a built-in kind `lesson`, and `curriculum_item`
orders it.

```
course             a post of the site's own course kind
  └ module         a heading, ordered            (module)
      └ lesson     a post of kind `lesson`       (curriculum_item)
          ├ video  one video asset               (video_asset)
          └ files  attachments                   (media, existing)
```

### New tables

```
video_asset      provider, provider_id, status, duration_seconds, thumbnail_url,
                 bytes, uploaded_at

module           course_post_id, title, position

curriculum_item  module_id, lesson_post_id, video_asset_id, position,
                 free_preview, unlock_after_days, unlock_on

student          email, password_hash (argon2, as users), name, locale,
                 created_at, last_seen_at, blocked
                 -- unique on email

student_session  id, student_id, expires_at, created_at
                 -- its own table; see §5

enrolment        student_id, course_post_id, granted_at, starts_at,
                 expires_at (nullable = forever), source, note
                 -- unique on (student, course)

lesson_progress  student_id, lesson_post_id, seconds_watched, completed_at,
                 last_position_seconds, updated_at
```

`expires_at` nullable is the "forever" case, and a nullable column is a better
answer than a sentinel date in 2099 that somebody will one day compare wrongly.

`source` on `enrolment` says how they got in — `manual`, `payment`, `coupon`,
`import`. It is the column that answers "why does this person have access", and
it is always the first question when something has gone wrong.

### Migrations

Per-tenant, as everything is. One migration file, the existing
`sea-orm-migration` pattern, and it runs when a site is next opened. Nothing
special — except that this is nine tables and it is worth one migration rather
than nine, so that a half-migrated site cannot exist.

---

## 5. A student is not a panel account

The one place to be inflexible.

The `user` table holds administrators, the `builder` account and the day-long
`assistant` account. Every one of them can read `/api/posts`. A student on a
three-month course must not be able to, ever, by any route — and the way to
guarantee that is not a role check that somebody will forget to add to the
fortieth endpoint. It is a **separate table, a separate session table, and a
separate cookie**, so that there is no code path from a student's credential to
a panel session at all.

```
mavicms_session    → user           → the panel and the API
mavicms_student    → student        → the member area, and nothing else
```

- argon2, the same as `users.rs` (`hash`/`verify` are already written).
- `throttle.rs` on the sign-in, as `/login` already has. A course site is
  worth guessing at.
- Passwords set by the student from a one-time link, not chosen by the
  administrator and sent by email. "Şifre verip" is what the customer asked
  for, and what they mean is *let this person in* — the account is created, the
  student gets a link, the link sets a password. Nobody types a password into a
  chat window.
- Optionally no password at all: a signed link to their email each time. Fewer
  support requests than any password policy, and the SES plugin is already
  there. Worth offering as the default.

---

## 6. Access: three months, and what happens on the last day

```
                 starts_at            expires_at
  ─────────────────┼────────────────────┼──────────────>
     no access     │      access        │   what?
```

**What happens after `expires_at`** is a product decision that has to be made
before the column is written, because it changes the answer to "can they still
see their notes":

- Locked: the course disappears from their list. Simplest, harshest.
- Read-only: they keep the notes and the completed marks, and the video stops.
  **This is the right default.** It is kinder, it makes renewal an obvious
  button rather than a support email, and it costs one branch.

Expiry is computed, never a job that sweeps rows. A row with `expires_at` in the
past *is* expired; nothing has to run for that to be true. (One job, daily: the
warning email at seven days and one day. That is mail, not state.)

### Drip

Two rules, and a course uses one or the other, not both:

- `unlock_after_days` — n days after **that student's** `starts_at`. A course
  sold as eight weeks, where everybody's week three is their own.
- `unlock_on` — a fixed date. A cohort that starts together in September.

A locked lesson is **visible and greyed with the date it opens**, not hidden.
Hidden lessons produce support tickets; a date produces patience.

---

## 7. The panel

New screens, in the order they are worth building:

| Screen | What it is |
|---|---|
| **Videos** | The library. Upload, transcode status, duration, which lessons use it. Beside Media, not inside it — the storage is a different provider and the sizes are different by three orders of magnitude. |
| **Courses → Curriculum** | Modules and lessons, dragged into order, a video picked per lesson, the drip rule set there. The one screen that has to be pleasant, because it is where a course is actually assembled. |
| **Students** | Search, add one, see what they are enrolled in and when it runs out. Bulk import from CSV, because everybody arrives with a spreadsheet. |
| **Enrolments** | Grant, extend, revoke. "Three months from today" as one button, not a date picker somebody miscounts. |
| **Progress** | Per course: who started, who finished, where people stop. The last of those is the only analytics that changes what a customer does next. |

Everything mobile-first, as the editor now is.

---

## 8. What the front end gets

The sales page is the customer's own Astro site and stays static — it should be
public and indexed, which is the point of a sales page.

```
GET  /courses                      the catalogue, public
GET  /courses/{slug}               one course, its curriculum, no video URLs
POST /students/session             sign in
POST /students/magic               ask for a link
GET  /students/me                  who, and what they are enrolled in
GET  /lessons/{id}                 notes + a playback URL, gated
POST /lessons/{id}/progress        seconds watched, last position
```

`GET /courses/{slug}` deliberately returns the curriculum **without** playback
URLs: the sales page needs to show what is in the course, and the video address
is the one thing it must not carry.

And `llms.txt` gets a section, so that an assistant handed the key from PR #22
can build the sales pages without being told any of this — which is now the way
this project expects front ends to get built.

---

## 9. Email

The SES plugin is already there, and `email::send` is a transactional path, not
just campaigns. Five messages:

1. You have been given access — with the link that sets a password.
2. Here is your sign-in link (if passwordless).
3. Lesson 4 is open now (drip, if the course wants it).
4. Your access ends in seven days. And: tomorrow.
5. Your access has ended, and how to renew.

Number 4 is the one that pays for the feature.

---

## 10. Payments

**Not in the first release.** Manual enrolment has to work with no payment
provider configured at all, because that is how the first customer will use it:
they sell by bank transfer or in person and let people in by hand.

When it comes, it is a plugin like the others:

- **Turkey: iyzico or PayTR.** 3D Secure, TRY, instalments — the instalment
  table is not optional for a ₺4,500 course in Turkey.
- **Elsewhere: Stripe.** Checkout, and a webhook that writes an `enrolment`
  with `source: payment`.

The webhook is the only thing that creates a paid enrolment. Never the browser
saying it succeeded.

---

## 11. For somebody who wants their own member area

The API in §8 is the whole of it. Nothing about the gate assumes our member
area is what is asking, so an Astro project running SSR can call it with the
student's cookie and render whatever it likes.

Say this out loud in the documentation but do not build for it: the customer who
wants this is one in twenty, and the other nineteen want the member area to
already exist.

---

## 12. Order of work

Each of these is shippable, and each one is worth having on its own.

**One — video that plays.** The video plugin with one provider, the Videos
screen, direct upload, the webhook, and a player behind a signed URL. No
courses, no students. Prove the hard part first: a two-gigabyte upload from a
Turkish office, and a URL that is dead tomorrow.

**Two — a course with lessons.** `lesson` as a built-in kind, modules,
curriculum, ordering. Still no students; an administrator previewing.

**Three — students and access.** The student table, sign-in, the member area,
manual enrolment with an end date, and read-only after expiry. **This is the
release the customer asked for.**

**Four — drip and progress.** Unlock rules, resume position, the completion
marks, the progress screen.

**Five — the emails.** Especially the expiry warning.

**Six — payments.** iyzico first, on the evidence that somebody will pay for it.

Two, three and four each need one, so one is not a phase to hurry.

---

## 13. What will bite

- **The upload webhook is an unauthenticated inbound address.** Same shape as
  the SES event address, which is already solved here with an unguessable
  token in the path (`events_token`). Do that, and verify the provider's
  signature as well where there is one.
- **Signed URLs leak into logs.** Short lifetimes are the answer; do not put a
  playback URL anywhere it will be written down.
- **A deleted video with lessons pointing at it.** Refuse it, the way a content
  type refuses to go while content uses it. The bin pattern applies: thirty
  days, and the provider's copy stays until then.
- **Storage cost is not the bill; delivery is.** One popular course, a thousand
  students, an hour a week each: on a per-GB provider that is a real number and
  it should be visible per site *before* it is a surprise. A usage figure in the
  panel from month one.
- **Two of everything.** Two session tables, two sign-in pages, two rate
  limiters, two password-reset flows. That duplication is the price of §5 and it
  is worth paying, but it should be written once and shared, not copied.
- **`expires_at` and time zones.** Everything already stores
  `DateTimeWithTimeZone`; a course that ends "on the 30th" ends at a moment,
  and the student's midnight is not the server's.
- **Video in the backup plugin.** The existing backup takes the database and
  the uploads. It cannot take two terabytes out of Bunny, and it should say so
  rather than appear to have done it.

---

## 14. Roughly what it costs to run

One site, ten courses, twenty hours of video, five hundred students watching an
hour a month:

| | Bunny | Cloudflare Stream |
|---|---|---|
| Storage, 20 h | ~$0.30 /mo | ~$6 /mo |
| Delivery, 500 h | ~$3–5 /mo | ~$30 /mo |

Both are small against a course that sells for anything at all, and the shape
is what matters: **delivery grows with students, storage does not.** A site with
one hit course pays for that course; a site with a hundred videos nobody watches
pays almost nothing. Bill it per site, and show it in the panel.
