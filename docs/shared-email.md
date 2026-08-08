# Lending one Amazon account to every site

A customer who wants their contact form to email them should not have to open
an AWS account first. So the server can hold one set of Amazon credentials and
let its sites send through it.

The whole of the difficulty is reputation. Amazon judges an account by what
leaves it, and a shared account means one site sending to a list somebody
bought puts everybody else's mail in spam. Amazon's own answer is a **tenant**:
a container for one sender's identities, with its own reputation and its own
sending status, which can be stopped without stopping anything else. Every send
the server makes on a site's behalf names one.

The second half is ours, because Amazon does not meter per tenant: each site
gets a number of messages a day, small to begin with.

## What to make in AWS

### 1. A user for the server

Create an IAM user with programmatic access and no console password. Its keys go
into **the server's console → Mail**, and nowhere else — never into a site's own
settings.

Attach this policy. It is the exact set of calls this code makes, and nothing
else:

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Sid": "Send",
      "Effect": "Allow",
      "Action": [
        "ses:SendEmail"
      ],
      "Resource": "*"
    },
    {
      "Sid": "SitesAndTheirDomains",
      "Effect": "Allow",
      "Action": [
        "ses:CreateEmailIdentity",
        "ses:DeleteEmailIdentity",
        "ses:GetEmailIdentity",
        "ses:ListEmailIdentities",
        "ses:PutEmailIdentityMailFromAttributes"
      ],
      "Resource": "*"
    },
    {
      "Sid": "Tenants",
      "Effect": "Allow",
      "Action": [
        "ses:CreateTenant",
        "ses:GetTenant",
        "ses:ListTenants",
        "ses:CreateTenantResourceAssociation"
      ],
      "Resource": "*"
    },
    {
      "Sid": "AccountAndTracking",
      "Effect": "Allow",
      "Action": [
        "ses:GetAccount",
        "ses:PutAccountDetails",
        "ses:PutAccountSendingAttributes",
        "ses:CreateConfigurationSet",
        "ses:ListConfigurationSets",
        "ses:CreateConfigurationSetEventDestination",
        "ses:BatchGetMetricData"
      ],
      "Resource": "*"
    },
    {
      "Sid": "WhoAmazonHasStoppedWritingTo",
      "Effect": "Allow",
      "Action": [
        "ses:ListSuppressedDestinations",
        "ses:DeleteSuppressedDestination"
      ],
      "Resource": "*"
    },
    {
      "Sid": "EventsComeBack",
      "Effect": "Allow",
      "Action": [
        "sns:CreateTopic",
        "sns:Subscribe"
      ],
      "Resource": "*"
    }
  ]
}
```

The same three steps from a shell, if you would rather not click. Save the
policy above as `ses-policy.json` first:

```sh
aws iam create-policy --policy-name mavicms-ses \
  --policy-document file://ses-policy.json
aws iam create-user --user-name mavicms-mail
aws iam attach-user-policy --user-name mavicms-mail \
  --policy-arn arn:aws:iam::<account>:policy/mavicms-ses
aws iam create-access-key --user-name mavicms-mail > key.json
```

The secret is in that file and nowhere else — Amazon will not show it again.
Copy it into the panel, then delete the file. Worth checking afterwards that
the policy is neither more nor less than it should be, which `simulate` will
answer without changing anything:

```sh
aws iam simulate-principal-policy \
  --policy-source-arn arn:aws:iam::<account>:user/mavicms-mail \
  --action-names ses:SendEmail ses:CreateTenant iam:CreateUser
```

The first two should come back `allowed` and the third `implicitDeny`.

`ses:DeleteTenant` is deliberately absent. A tenant carries a site's reputation
history, and there is no button in this panel that removes one — a site that
should stop sending is set to nought messages a day instead.

The Support permissions (`support:CreateCase`, `support:DescribeCases`) are only
needed if you want to ask Amazon for production access or a higher quota from
inside the panel. They require a Business or Enterprise support plan; without
one those two screens will say so, and everything else works.

### 2. Out of the sandbox

A new SES account may only send 200 messages a day, and only to addresses it has
verified. That is a sandbox account, and lending it to sites is pointless: their
customers' addresses are not verified.

Ask Amazon for production access — from the SES console, or from **Mail →
Production access** in the panel if you have a support plan. Say what the mail
is (transactional notifications and opt-in newsletters for hosted sites), how
people get on the lists, and how they get off. Amazon usually answers within a
day.

Until they do, the panel will show the account's real limit, and the numbers you
hand out to sites should stay under it.

### 3. The region

Pick one and keep it. An identity verified in `eu-central-1` is not verified in
`eu-west-1`, and moving means every customer adds their DNS records again.

## Whose address the mail comes from

The server lends the account, not the name on the letter. A site borrowing the
account still sends **as itself** — the credentials, the region and the tenant
come from the server, and the From address, the reply address and the sender
list come from the site's own settings.

A site that has not named a sender yet still sends, under the server's address,
with replies pointed back at the site. That is deliberate: a contact form that
fails silently until somebody edits DNS is worse than one that works and says
where the mail is coming from. The panel says which of the two is happening,
and the difference is one field.

So neither is compulsory. A customer who never touches DNS gets working mail
under the host's name; one who publishes the records gets their own. What a
customer cannot do is send as a domain somebody else added — the first site to
add a domain owns it, and a site may only list, alter and remove the senders it
added itself. On a shared account that is not a nicety: the SES identity list is
the whole server's, so without it one customer would see every other customer's
domains and could delete them.

## What a customer adds to their DNS

A site sending through the server's account still sends **as its own domain**,
so Amazon has to be shown that the domain agrees. That is what these records
are, and the panel lists them with the exact values once the domain is added
under **Mail → Senders**.

| Record | What it is |
|---|---|
| Three `CNAME`s ending in `dkim.amazonses.com` | DKIM. It signs the mail, so a receiving server can tell it really came from this domain. Without it almost everything lands in spam. |
| One `MX` on `send.<domain>` (optional) | A custom MAIL FROM. It makes the invisible envelope address belong to the customer's domain too, which is what SPF alignment needs. |
| One `TXT` on the same subdomain, `v=spf1 include:amazonses.com ~all` | Goes with the MX above. |
| One `TXT` on `_dmarc.<domain>` | DMARC. Start with `v=DMARC1; p=none; rua=mailto:…` and tighten it later. Gmail and Yahoo have required a DMARC record of bulk senders since February 2024. |

Two things that go wrong while adding them:

- **The MAIL FROM subdomain has to be a subdomain.** A domain that already
  receives mail has an `MX` at its apex, and putting Amazon's there instead
  takes the receiving with it. That is why the record goes on `mail.` or
  `send.` and never on the domain itself.
- **Most DNS panels append the zone to whatever is typed in the name field.**
  Pasting the whole record name produces
  `…_domainkey.example.com.example.com`, which resolves to nothing and leaves
  the domain pending with no indication of why.

Two things worth telling a customer plainly:

- **The DKIM records are tied to the account that issued them.** They verify the
  domain inside *this* server's Amazon account. If the site later moves to its
  own AWS account, it gets three new records and replaces them. Nothing breaks
  in the meantime, but it is not a one-time task they can forget.
- **Adding the records is the whole of the work.** There is no waiting on us:
  the panel re-asks Amazon and shows verified when the records have propagated,
  usually within the hour and occasionally the next day.

## What the server decides

In the console, under **Mail**, each site shows:

- whether it sends with **its own** Amazon account or **the server's**
- how many messages a day it is allowed, and how many it has sent today
- what Amazon says about that site's own tenant

A site starts at **200 a day**. That is a contact form and some notifications
and not a mailing list, and it is enough to find out what a site is before
trusting it with more. Nought stops a site without taking anything away from it:
the settings, the lists and the history stay, and raising the number starts it
again.

A site's own keys always win. Somebody who went to the trouble of putting them
in meant to use them, so the server's account is the fallback and never an
override.

## What to watch

- **The sum of what you hand out should stay under the account's own limit.**
  The console shows both. Amazon's limit is per account; the tenants divide the
  reputation, not the quota.
- **A tenant Amazon has paused shows as such in the console.** That site is the
  only one affected, which is the entire reason for the arrangement — but
  somebody still has to look, or find out from the customer.
- **Complaints are the thing that ends accounts, not bounces.** A site whose
  recipients press "spam" is a site to talk to before Amazon does.
