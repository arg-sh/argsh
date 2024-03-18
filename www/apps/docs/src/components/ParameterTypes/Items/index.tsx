import {
  CopyButton,
  DetailsSummary,
  ExpandableNotice,
  FeatureFlagNotice,
  InlineCode,
  MarkdownContent,
} from "docs-ui"
import React, { useEffect, useMemo, useRef, useState } from "react"
import Details from "../../../theme/Details"
import clsx from "clsx"
import { Parameter } from ".."
import {
  ArrowDownLeftMini,
  ArrowsPointingOutMini,
  Link,
  TriangleRightMini,
} from "@medusajs/icons"
import IconFlagMini from "../../../theme/Icon/FlagMini"
import decodeStr from "../../../utils/decode-str"
import { useLocation } from "@docusaurus/router"
import useDocusaurusContext from "@docusaurus/useDocusaurusContext"
import isInView from "../../../utils/is-in-view"

type CommonProps = {
  level?: number
  expandUrl?: string
  sectionTitle?: string
}

type ParameterTypesItemProps = {
  parameter: Parameter
  elementKey: number
} & CommonProps &
  React.AllHTMLAttributes<HTMLDivElement>

const ParameterTypesItem = ({
  parameter,
  level = 1,
  expandUrl,
  elementKey,
  sectionTitle,
}: ParameterTypesItemProps) => {
  const location = useLocation()

  const {
    siteConfig: { url },
  } = useDocusaurusContext()

  const groupName = useMemo(() => {
    switch (level) {
      case 1:
        return "group/parameterOne"
      case 2:
        return "group/parameterTwo"
      case 3:
        return "group/parameterThree"
      case 4:
        return "group/parameterFour"
    }
  }, [level])
  const borderForGroupName = useMemo(() => {
    switch (level) {
      case 1:
        return "group-open/parameterOne:border-solid group-open/parameterOne:border-0 group-open/parameterOne:border-b"
      case 2:
        return "group-open/parameterTwo:border-solid group-open/parameterTwo:border-0 group-open/parameterTwo:border-b"
      case 3:
        return "group-open/parameterThree:border-solid group-open/parameterThree:border-0 group-open/parameterThree:border-b"
      case 4:
        return "group-open/parameterFour:border-solid group-open/parameterFour:border-0 group-open/parameterFour:border-b"
    }
  }, [level])
  const rotateForGroupName = useMemo(() => {
    switch (level) {
      case 1:
        return "group-open/parameterOne:rotate-90"
      case 2:
        return "group-open/parameterTwo:rotate-90"
      case 3:
        return "group-open/parameterThree:rotate-90"
      case 4:
        return "group-open/parameterFour:rotate-90"
    }
  }, [level])
  function getItemClassNames(details = true) {
    return clsx(
      "odd:[&:not(:first-child):not(:last-child)]:!border-y last:not(:first-child):!border-t",
      "first:!border-t-0 first:not(:last-child):!border-b last:!border-b-0 even:!border-y-0",
      details && groupName,
      !details && borderForGroupName
    )
  }
  const formatId = (str: string) => {
    return str.replaceAll(" ", "_")
  }
  const parameterId = useMemo(() => {
    return sectionTitle
      ? `#${formatId(sectionTitle)}-${formatId(
          parameter.name
        )}-${level}-${elementKey}`
      : ""
  }, [sectionTitle, parameter, elementKey])
  const parameterPath = useMemo(
    () => `${location.pathname}${parameterId}`,
    [location, parameterId]
  )
  const parameterUrl = useMemo(
    () => `${url}${parameterPath}`,
    [url, parameterPath]
  )

  const ref = useRef<HTMLDivElement>()
  const [isSelected, setIsSelected] = useState(false)

  useEffect(() => {
    if (!parameterId.length) {
      return
    }

    const shouldScroll = location.hash === parameterId
    if (shouldScroll && !isSelected && ref.current && !isInView(ref.current)) {
      ref.current.scrollIntoView({
        block: "center",
      })
    }

    setIsSelected(shouldScroll)
  }, [parameterId])

  function getSummary(parameter: Parameter, nested = true) {
    return (
      <DetailsSummary
        subtitle={
          parameter.description || parameter.defaultValue ? (
            <>
              <MarkdownContent
                allowedElements={["a", "strong", "code", "ul", "ol", "li"]}
                unwrapDisallowed={true}
                className="text-medium"
              >
                {parameter.description}
              </MarkdownContent>
              {parameter.defaultValue && (
                <p className="mt-0.5 mb-0">
                  Default: <InlineCode>{parameter.defaultValue}</InlineCode>
                </p>
              )}
            </>
          ) : undefined
        }
        expandable={parameter.children?.length > 0}
        hideExpandableIcon={true}
        className={clsx(
          getItemClassNames(false),
          "py-1 pr-1",
          level === 1 && "pl-1",
          level === 2 && "pl-3",
          level === 3 && "pl-[120px]",
          level === 4 && "pl-[160px]",
          !nested && "cursor-default",
          isSelected && "animate-flash animate-bg-surface"
        )}
        onClick={(e) => {
          const targetElm = e.target as HTMLElement
          if (targetElm.tagName.toLowerCase() === "a") {
            window.location.href =
              targetElm.getAttribute("href") || window.location.href
            return
          }
        }}
        summaryRef={!nested ? ref : undefined}
        id={!nested && parameterId ? parameterId : ""}
      >
        <div className="flex gap-0.5">
          {nested && (
            <TriangleRightMini
              className={clsx(
                "text-medusa-fg-subtle transition-transform",
                rotateForGroupName
              )}
            />
          )}
          {!nested && level > 1 && (
            <ArrowDownLeftMini
              className={clsx("text-medusa-fg-subtle flip-y")}
            />
          )}
          {level === 1 && parameterId.length > 0 && (
            <CopyButton
              text={parameterUrl}
              onCopy={(e: React.MouseEvent<HTMLSpanElement, MouseEvent>) => {
                e.preventDefault()
                e.stopPropagation()
              }}
            >
              <Link
                className={clsx(
                  "text-medusa-fg-interactive hover:text-medusa-fg-interactive-hover"
                )}
              />
            </CopyButton>
          )}
          <div className="flex gap-0.75 flex-wrap">
            <InlineCode>{decodeStr(parameter.name)}</InlineCode>
            <span className="font-monospace text-compact-small-plus text-medusa-fg-subtle">
              <MarkdownContent allowedElements={["a"]} unwrapDisallowed={true}>
                {parameter.type}
              </MarkdownContent>
            </span>
            {parameter.optional === false && (
              <span
                className={clsx(
                  "text-compact-x-small-plus uppercase",
                  "text-medusa-fg-error"
                )}
              >
                Required
              </span>
            )}
            {parameter.featureFlag && (
              <FeatureFlagNotice
                featureFlag={parameter.featureFlag}
                type="parameter"
                badgeClassName="!p-0 leading-none"
                badgeContent={
                  <IconFlagMini className="!text-medusa-tag-green-text" />
                }
              />
            )}
            {parameter.expandable && (
              <ExpandableNotice
                type="method"
                link={expandUrl || "#"}
                badgeClassName="!p-0 leading-none"
                badgeContent={<ArrowsPointingOutMini />}
              />
            )}
          </div>
        </div>
      </DetailsSummary>
    )
  }

  return (
    <>
      {parameter.children?.length > 0 && (
        <Details
          summary={getSummary(parameter)}
          className={clsx(getItemClassNames())}
          heightAnimation={true}
          ref={ref}
          id={parameterId ? parameterId : ""}
        >
          {parameter.children && (
            <ParameterTypesItems
              parameters={parameter.children}
              level={level + 1}
              expandUrl={expandUrl}
            />
          )}
        </Details>
      )}
      {(parameter.children?.length || 0) === 0 && getSummary(parameter, false)}
    </>
  )
}

type ParameterTypesItemsProps = {
  parameters: Parameter[]
} & CommonProps

const ParameterTypesItems = ({
  parameters,
  ...rest
}: ParameterTypesItemsProps) => {
  return (
    <div>
      {parameters.map((parameter, key) => (
        <ParameterTypesItem
          parameter={parameter}
          key={key}
          elementKey={key}
          {...rest}
        />
      ))}
    </div>
  )
}

export default ParameterTypesItems
