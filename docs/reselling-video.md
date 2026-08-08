# Buying video wholesale and selling it on

The question is whether to hold the account with Bunny or Cloudflare yourself
and resell capacity to the sites on this server, rather than each of them
bringing their own. The plugin as written supports both — the settings are per
site, and a site that has its own credentials uses them. What follows is what
has to be true for the wholesale version to make money rather than lose it
quietly.

Prices are from memory, in the ranges the two providers published. Check them
before you price anything.

---

## 1. What you would actually be buying

Video has two costs and they behave completely differently.

|  | grows with | who controls it |
|---|---|---|
| **Storage** | how much they upload | the customer, once |
| **Delivery** | how much gets watched | the customer's students, every month, without limit |

Storage is boring: predictable, small, and it stops growing when they stop
uploading. **Delivery is the whole business risk.** It has no ceiling, the
customer does not feel it, and one course that goes well can multiply it by
twenty in a week.

Roughly, per month:

|  | Bunny Stream | Cloudflare Stream |
|---|---|---|
| Storage | ~$0.005 /GB | ~$5 per 1,000 minutes stored |
| Delivery | ~$0.005–0.01 /GB | ~$1 per 1,000 minutes delivered |

The units differ, and that is not a detail — it decides which one you can
resell safely. **Cloudflare bills by the minute watched.** A minute watched is a
minute watched whether the student is on a phone at 480p or a desk at 1080p, so
your cost per student-hour is a fixed number you can put in a contract.
**Bunny bills by the gigabyte**, so the same hour costs you three times more at
1080p than at 480p, and the customer chooses.

For reselling, that asymmetry matters more than the headline price.

---

## 2. The unit you sell

Do not sell "unlimited". Do not sell "storage". Sell the thing your cost is
made of:

> **Watch-hours.** One student watching one hour.

- It is what the customer already thinks in. "Three hundred students, four
  hours of course, most will finish" is a number they can produce.
- It maps one-to-one onto Cloudflare's billing and closely enough onto Bunny's
  (about 0.5–1.5 GB per hour depending on the resolution you allow).
- It scales with their revenue, which is the only kind of cost a customer
  accepts without argument.

Sell storage as a much smaller line, or fold it into the plan, because it will
never be the bill.

---

## 3. What it costs you, per watch-hour

Take Cloudflare at $1 per 1,000 minutes delivered:

```
1 watch-hour = 60 minutes = $0.06
```

Bunny, at $0.01/GB and about 1 GB per hour at 720p:

```
1 watch-hour ≈ $0.01
```

Six times cheaper — and six times more exposed to a customer who uploads
everything at 1080p and lets students watch on a 4K monitor.

A worked plan, on Cloudflare, with a 3× markup:

| | |
|---|---|
| Included | 2,000 watch-hours / month |
| Your cost | ~$120 |
| Sell at | ~$350–400 |
| Overage | $0.18 / watch-hour |

The 3× is not greed. It is what covers the customer who uses 2,000 hours in
the first three days of a launch, the one who never uses any, the payment
processor, and the month a provider changes its pricing.

**On Bunny the same plan costs you ~$20 and you can sell it at $150** — which
is the argument for Bunny, and the reason to also cap the resolution.

---

## 4. The five things that must exist before you sell any of it

Without these, reselling is a way of buying video for other people.

### 4.1 Metering, per site, daily

Not "we can query the provider when somebody asks". A row per site per day,
written by a job, kept for at least fourteen months:

```
video_usage   day, delivered_seconds, delivered_bytes, stored_seconds,
              stored_bytes
```

Daily, because a monthly total tells you a customer overran and not when, and
because the provider's own history does not go back far enough to argue with.
Fourteen months, because the second year's renewal conversation is about last
year's same month.

The panel already reads storage — the durations of the videos a site holds.
Delivery has to come from the provider's statistics API, per library (Bunny)
or filtered by `creator` (Cloudflare), which is the reason for §4.2.

### 4.2 One tenant, one bucket the provider can tell apart

**This is the decision that is hard to undo.** If every site's videos go into
one Bunny library or one Cloudflare account with nothing distinguishing them,
you cannot bill any of them, and you find out in month two.

- **Bunny:** one **video library per site**. The API can make them, so it is a
  step in "add a site" rather than a manual chore. Statistics are per library,
  which is exactly the number you need.
- **Cloudflare:** one account, and the `creator` field set to the site's id on
  every upload. Analytics filter by it. Cheaper to operate than an account per
  site, and enough to bill on.

Both are a small amount of code *now* and a migration of everybody's videos
*later*.

### 4.3 A ceiling that actually stops

A plan with an overage rate and no hard limit is an unbounded liability written
in a friendly tone. Three levels, and the customer sees all three:

1. **80% of the allowance** — an email. Nothing else happens.
2. **100%** — overage begins, at the published rate, and the panel says so on
   every page.
3. **A hard multiple — 3× or 5× the allowance** — playback stops for new
   sessions and the site owner gets told why.

Level 3 will feel harsh right up to the first time somebody's course is
embedded on a forum with forty thousand readers.

### 4.4 A resolution cap

On Bunny, the difference between 720p and 4K is a factor of eight on the bill
and no difference at all to somebody watching a person talk over slides.
**Cap at 1080p by default**, offer higher as a paid option, and say so.

This single setting is worth more than any negotiation with the provider.

### 4.5 Your own margin, visible to you and not to them

The panel should show a customer **watch-hours used against their allowance**.
It should show *you* — in the console, not the site panel — cost, revenue and
margin per site per month. The customer who is unprofitable is never the one
you expect, and you will not find them by looking at invoices.

---

## 5. Where the money actually goes wrong

**One customer with a hit course.** Ninety per cent of your delivery bill will
be one or two sites. Price so that the median customer is very profitable,
because the mean one will not be.

**A course sold once and watched for ever.** They pay you monthly; the students
who bought two years ago still watch. Either access expires — which the plan in
`courses.md` builds — or your cost keeps rising against revenue that stopped.
This is the single most common way a course platform loses money, and the fix
is a product decision, not a pricing one.

**Storage that nobody watches.** Old course versions, re-recordings, mistakes.
Charge a little for storage anyway — not because it costs much, but because
free storage is never cleaned up.

**Refunds and chargebacks after the video was delivered.** You cannot get the
bandwidth back.

**A provider's price change.** Contract annually with the customer and monthly
with the provider, never the reverse.

---

## 6. Should you actually do it?

**Yes, if** you are already the one running the server for these sites. The
margin is real, the customer is spared an account and a card, and — the part
that is easy to undervalue — **they cannot leave for a competitor without also
moving their video**, which is a week of work they will not do casually.

**No, if** they are technical enough to hold their own account and you would
be reselling at a markup they can see. An agency with five clients will do the
arithmetic and be annoyed by it.

**The version that is nearly always right:** support both, which is what the
plugin already does. Bring-your-own for the customers who want it; yours as the
default, because the default is what almost everybody takes. Then the wholesale
account is an upsell rather than a toll, and the ones who would have resented it
never meet it.

---

## 7. In order

1. **Per-site library or `creator` tag on every upload.** Before anybody's
   second video, because retrofitting it means moving everybody's.
2. **Daily usage rows**, and the panel showing watch-hours used.
3. **Allowance, warning email, overage.**
4. **The hard ceiling**, which nobody wants to build and everybody needs.
5. **Margin per site**, in the console.
6. Only then, plans and a price list.

One to three is a week. Four to five is another. Selling before four exists is
where the money goes.
