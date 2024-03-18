import React from "react"
import { Badge, Link, Tooltip } from "@/components"

export type FeatureFlagNoticeProps = {
  featureFlag: string
  type?: "endpoint" | "parameter"
  tooltipTextClassName?: string
  badgeClassName?: string
  badgeContent?: React.ReactNode
}

export const FeatureFlagNotice = ({
  featureFlag,
  type = "endpoint",
  tooltipTextClassName,
  badgeClassName,
  badgeContent = "feature flag",
}: FeatureFlagNoticeProps) => {
  return (
    <Tooltip
      tooltipChildren={
        <span className={tooltipTextClassName}>
          To use this {type}, make sure to
          <br />
          <Link
            href="https://arg.sh/development/feature-flags/toggle"
            target="__blank"
          >
            enable its feature flag: <code>{featureFlag}</code>
          </Link>
        </span>
      }
      clickable
    >
      <Badge variant="green" className={badgeClassName}>
        {badgeContent}
      </Badge>
    </Tooltip>
  )
}
