import {
  AnalyticsProvider,
  ColorModeProvider,
  ModalProvider,
  NotificationProvider,
} from "docs-ui"
import React from "react"
import { useThemeConfig } from "@docusaurus/theme-common"
import { ThemeConfig } from "@medusajs/docs"
import SearchProvider from "../Search"
import SkipToContent from "@theme/SkipToContent"

type DocsProvidersProps = {
  children?: React.ReactNode
}

const DocsProviders = ({ children }: DocsProvidersProps) => {
  const {
    analytics: { apiKey },
  } = useThemeConfig() as ThemeConfig

  return (
    <AnalyticsProvider writeKey={apiKey}>
      <ColorModeProvider>
        <ModalProvider>
          <SearchProvider>
            <NotificationProvider>
              <SkipToContent />
              {children}
            </NotificationProvider>
          </SearchProvider>
        </ModalProvider>
      </ColorModeProvider>
    </AnalyticsProvider>
  )
}

export default DocsProviders
