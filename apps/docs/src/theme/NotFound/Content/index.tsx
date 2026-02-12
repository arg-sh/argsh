import React from "react"
import clsx from "clsx"
import Translate from "@docusaurus/Translate"
import type { Props } from "@theme/NotFound/Content"
import Heading from "@theme/Heading"
import Link from "@docusaurus/Link"

export default function NotFoundContent({ className }: Props): JSX.Element {
  return (
    <main
      className={clsx("container markdown theme-doc-markdown my-4", className)}
    >
      <div className="row">
        <div className="col col--6 col--offset-3">
          <Heading as="h1">
            <Translate
              id="theme.NotFound.title"
              description="The title of the 404 page"
            >
              Page Not Found
            </Translate>
          </Heading>
          <p>
            <Translate
              id="theme.NotFound.p1"
              description="The first paragraph of the 404 page"
            >
              Looks like the page you&apos;re looking for has either changed
              into a different location or isn&apos;t in our documentation
              anymore.
            </Translate>
          </p>
          <p>
            If you think this is a mistake, please{" "}
            <Link
              href="https://github.com/arg-sh/argsh/issues/new?assignees=&labels=type%3A+docs&template=docs.yml"
              rel="noopener noreferrer"
              target="_blank"
            >
              report this issue on GitHub
            </Link>
          </p>
        </div>
      </div>
    </main>
  )
}
