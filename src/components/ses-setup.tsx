import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { Check, ChevronDown, Copy, ExternalLink } from "lucide-react"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible"

/**
 * How to get the keys this plugin wants, without leaving the page to find out.
 *
 * The permissions are the part people get wrong: SES has one action for
 * sending and eleven more for everything else on this page, and a key made
 * from the obvious instruction can send mail while every other panel here
 * refuses. So the policy is written out ready to paste rather than described.
 *
 * The console links carry the region that has been typed in above, because
 * every SES screen is per-region and landing on the wrong one is how somebody
 * verifies an address that then cannot send.
 */

/**
 * Everything the plugin calls, in blocks that can be deleted whole.
 *
 * Somebody who does not want this panel opening support cases can drop that
 * last block and lose exactly one feature, which is why they are separate
 * statements rather than one list.
 */
const POLICY = `{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Sid": "SendMail",
      "Effect": "Allow",
      "Action": ["ses:SendEmail"],
      "Resource": "*"
    },
    {
      "Sid": "ReadTheAccount",
      "Effect": "Allow",
      "Action": [
        "ses:GetAccount",
        "ses:GetSendStatistics",
        "ses:ListConfigurationSets",
        "ses:ListEmailIdentities",
        "ses:GetEmailIdentity",
        "ses:ListSuppressedDestinations"
      ],
      "Resource": "*"
    },
    {
      "Sid": "ManageSenders",
      "Effect": "Allow",
      "Action": [
        "ses:CreateEmailIdentity",
        "ses:DeleteEmailIdentity",
        "ses:DeleteSuppressedDestination"
      ],
      "Resource": "*"
    },
    {
      "Sid": "AskAmazonForMore",
      "Effect": "Allow",
      "Action": [
        "ses:PutAccountDetails",
        "ses:PutAccountSendingAttributes"
      ],
      "Resource": "*"
    },
    {
      "Sid": "OpenASupportCase",
      "Effect": "Allow",
      "Action": ["support:CreateCase", "support:DescribeCases"],
      "Resource": "*"
    }
  ]
}`

function Away({ href, children }: { href: string; children: React.ReactNode }) {
  return (
    <a
      href={href}
      target="_blank"
      rel="noreferrer noopener"
      className="inline-flex items-center gap-1 underline underline-offset-2 hover:text-foreground"
    >
      {children}
      <ExternalLink className="size-3 shrink-0" />
    </a>
  )
}

export function SesSetupGuide({ region }: { region: string }) {
  const { t } = useLingui()
  const [copied, setCopied] = React.useState(false)

  // Every SES screen belongs to one region. Falling back to the one most
  // people pick is better than a link that opens the wrong account view.
  const where = region.trim() || "eu-central-1"
  const ses = `https://${where}.console.aws.amazon.com/ses/home?region=${where}`

  const copy = () => {
    void navigator.clipboard.writeText(POLICY).then(
      () => {
        setCopied(true)
        setTimeout(() => setCopied(false), 1500)
      },
      () => toast.error(t`Could not copy it`)
    )
  }

  return (
    <Collapsible className="rounded-xl border">
      <CollapsibleTrigger
        render={
          <button
            type="button"
            className="group flex w-full items-center gap-2 px-4 py-3 text-left text-sm font-medium"
          />
        }
      >
        <ChevronDown className="size-4 transition-transform group-data-[panel-open]:rotate-180" />
        {t`How to get these keys`}
      </CollapsibleTrigger>

      <CollapsibleContent>
        <div className="flex flex-col gap-5 border-t px-4 py-4 text-sm">
          <ol className="flex list-decimal flex-col gap-4 pl-5">
            <li>
              <p>
                {t`Make an IAM user in the AWS account that owns the sending domain.`}
              </p>
              <p className="mt-1 text-muted-foreground">
                <Away href="https://console.aws.amazon.com/iam/home#/users">
                  {t`IAM users`}
                </Away>
              </p>
            </li>

            <li>
              <p>{t`Give it this policy.`}</p>
              <div className="relative mt-2">
                <pre className="max-h-72 overflow-auto rounded-lg border border-border bg-muted/40 px-3 py-2 pr-12 text-xs">
                  {POLICY}
                </pre>
                <Button
                  variant="ghost"
                  size="icon-sm"
                  aria-label={t`Copy`}
                  className="absolute top-2 right-2"
                  onClick={copy}
                >
                  {copied ? <Check /> : <Copy />}
                </Button>
              </div>
              <p className="mt-2 text-muted-foreground">
                {t`The first block is all that sending needs; the rest is what this page reads and asks for. Any block can be left out — you lose that part of the page and nothing else.`}
              </p>
            </li>

            <li>
              <p>
                {t`Make an access key for that user and paste the two halves above, with the region your SES lives in.`}
              </p>
            </li>

            <li>
              <p>
                {t`Verify the address or domain you send from. SES refuses to send from anything it has not verified.`}
              </p>
              <p className="mt-1 text-muted-foreground">
                <Away href={`${ses}#/verified-identities`}>
                  {t`Verified identities in ${where}`}
                </Away>
              </p>
            </li>

            <li>
              <p>
                {t`Ask to leave the sandbox, from the section below. Until Amazon agrees, SES will only write to addresses you have verified — so a contact form can be tested but not used.`}
              </p>
            </li>
          </ol>

          <div className="flex flex-col gap-2 border-t border-border pt-4">
            <p className="font-medium">{t`Amazon's own pages`}</p>
            <ul className="flex flex-col gap-1 text-muted-foreground">
              <li>
                <Away href="https://docs.aws.amazon.com/ses/latest/dg/request-production-access.html">
                  {t`Leaving the sandbox`}
                </Away>
              </li>
              <li>
                <Away href="https://docs.aws.amazon.com/ses/latest/dg/creating-identities.html">
                  {t`Verifying an address or a domain`}
                </Away>
              </li>
              <li>
                <Away href="https://docs.aws.amazon.com/ses/latest/dg/manage-sending-quotas.html">
                  {t`Sending limits, and raising them`}
                </Away>
              </li>
              <li>
                <Away href="https://docs.aws.amazon.com/ses/latest/dg/faqs-enforcement.html">
                  {t`What Amazon does about bounces and complaints`}
                </Away>
              </li>
              <li>
                <Away href="https://docs.aws.amazon.com/ses/latest/dg/monitor-sending-activity.html">
                  {t`Watching how sending is going`}
                </Away>
              </li>
              <li>
                <Away href="https://docs.aws.amazon.com/ses/latest/dg/control-user-access.html">
                  {t`Permissions in SES`}
                </Away>
              </li>
              <li>
                <Away href="https://docs.aws.amazon.com/awssupport/latest/user/case-management.html">
                  {t`Support cases, which a quota increase is`}
                </Away>
              </li>
            </ul>
          </div>

          <div className="flex flex-wrap gap-4 border-t border-border pt-4 text-muted-foreground">
            <Away href={`${ses}#/account`}>{t`SES console`}</Away>
            <Away href="https://console.aws.amazon.com/support/home#/case/create">
              {t`Open a case at AWS`}
            </Away>
          </div>
        </div>
      </CollapsibleContent>
    </Collapsible>
  )
}
