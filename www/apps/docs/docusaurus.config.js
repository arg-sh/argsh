/* eslint-disable @typescript-eslint/no-var-requires */
import "dotenv/config"
import fs from "fs"
import path from "path"
import { themes as prismThemes } from "prism-react-renderer"
const reverseSidebarItems = require("./src/utils/reverse-sidebar")
const excludeSidebarResults = require("./src/utils/exclude-sidebar-results")

const announcementBar = JSON.parse(fs.readFileSync("./announcement.json"))

/** @type {import('@medusajs/docs').MedusaDocusaurusConfig} */
const config = {
  title: "Arg.sh",
  tagline: "Explore and learn how to use arg.sh",
  url: "https://arg.sh",
  baseUrl: "/",
  onBrokenLinks: "throw",
  onBrokenMarkdownLinks: "throw",
  favicon: "img/favicon.ico",
  organizationName: "argsh",
  projectName: "argsh/www",
  markdown: {
    mdx1Compat: {
      comments: true,
      admonitions: false,
      headingIds: false,
    },
    mermaid: true,
  },
  plugins: [
    require.resolve("docusaurus-plugin-image-zoom"),
    async function tailwindPlugin() {
      return {
        name: "docusaurus-tailwindcss",
        configurePostCss(postcssOptions) {
          // Appends TailwindCSS and AutoPrefixer.
          postcssOptions.plugins.push(require("tailwindcss"))
          postcssOptions.plugins.push(require("autoprefixer"))
          return postcssOptions
        },
      }
    },
    function webpackPlugin() {
      return {
        name: "custom-webpack-plugin",
        configureWebpack() {
          return {
            devServer: {
              client: {
                overlay: {
                  runtimeErrors: (error) => {
                    if (
                      error.message ===
                      "ResizeObserver loop completed with undelivered notifications."
                    ) {
                      return false
                    }
                    return true
                  },
                },
              },
            },
          }
        },
      }
    },
    [
      "./src/plugins/docusaurus-plugin-diagram2code-showcase",
      {
        directoryPath: path.join(__dirname, "diagrams"),
        outputPath: path.join(__dirname, "src", "utils"),
      },
    ],
  ],
  themes: ["@docusaurus/theme-mermaid"],
  themeConfig: {
    mermaid: {
      theme: {
        light: "base",
        dark: "base",
      },
      options: {
        themeVariables: {
          background: "#FFFFFF",
          mainBkg: "#FFFFFF",
          primaryColor: "#FFFFFF",
          primaryTextColor: "#030712",
          primaryBorderColor: "#D1D5DB",
          nodeBorder: "#D1D5DB",
          lineColor: "#11181C",
          fontFamily: "Inter",
          fontSize: "13px",
          tertiaryColor: "#F3F4F6",
          tertiaryBorderColor: "#D1D5DB",
          tertiaryTextColor: "#030712",
          clusterBkg: "#F3F4F6",
        },
      },
    },
    image: "img/docs-meta.jpg",
    colorMode: {
      defaultMode: "light",
      disableSwitch: false,
      respectPrefersColorScheme: true,
    },
    algoliaConfig: {
      appId: process.env.ALGOLIA_APP_ID || "temp",
      apiKey: process.env.ALGOLIA_API_KEY || "temp",
      indexNames: {
        docs: process.env.DOCS_ALGOLIA_INDEX_NAME,
        api: process.env.API_ALGOLIA_INDEX_NAME,
      },
      filters: [
        {
          value: "docs",
          label: "Docs",
        },
      ],
      defaultFiltersByPath: [],
      defaultFilters: ["docs"],
    },
    analytics: {
      apiKey: process.env.SEGMENT_API_KEY || "temp",
    },
    prism: {
      defaultLanguage: "ts",
      additionalLanguages: ["bash", "json"],
      plugins: ["line-numbers", "show-language"],
      theme: {
        ...prismThemes.vsDark,
        plain: {
          ...prismThemes.vsDark.plain,
          backgroundColor: "#111827",
        },
      },
    },
    zoom: {
      selector: ".markdown :not(.no-zoom-img) > img:not(.no-zoom-img)",
    },
    navbar: {
      hideOnScroll: false,
      logo: {
        alt: "arg.sh Logo",
        src: "img/logo-icon.svg",
        srcDark: "img/logo-icon-dark.svg",
        width: 20,
        height: 20,
      },
      items: [
        {
          type: "docSidebar",
          sidebarId: "homepage",
          label: "Docs",
          position: "left",
        },
        {
          type: "search",
          position: "right",
        },
      ],
    },
    navbarActions: [
      {
        type: "button",
        label: "Report an Issue",
        className: "max-[1014px]:hidden",
        href: "https://github.com/arg-sh/argsh/issues/new?assignees=&labels=type%3A+docs&template=docs.yml",
      },
    ],
    mobileLogo: {
      alt: "arg.sh",
      src: "img/logo-mobile.png",
      srcDark: "img/logo-mobile-dark.png",
      width: 82,
      height: 20,
    },
    footer: {
      copyright: `© ${new Date().getFullYear()} Jan Guth (@fentas). All rights reserved.`,
    },
    socialLinks: [
      {
        type: "github",
        href: "https://github.com/arg-sh/argsh",
      },
    ],
    reportCodeLinkPrefix:
      "https://github.com/fentas/argsg/issues/new?assignees=&labels=type%3A+docs&template=docs.yml",
    footerFeedback: {
      event: "survey",
    },
    docs: {
      sidebar: {
        hideable: true,
        autoCollapseCategories: true,
      },
    },
    cloudinaryConfig: {
      cloudName: process.env.CLOUDINARY_CLOUD_NAME || "",
      flags: ["fl_lossy", "f_auto"],
      resize: {
        action: "pad",
        aspectRatio: "16:9",
      },
      roundCorners: 16,
    },
  },
  presets: [
    [
      "@docusaurus/preset-classic",
      {
        docs: {
          sidebarPath: require.resolve("./sidebars.js"),
          editUrl: "https://github.com/arg-sh/argsh/edit/develop/www/apps/docs",
          path: "content",
          routeBasePath: "/",
          remarkPlugins: [
            [require("@docusaurus/remark-plugin-npm2yarn"), { sync: true }],
          ],
          showLastUpdateTime: true,
          // breadcrumbs: false,
          async sidebarItemsGenerator({
            defaultSidebarItemsGenerator,
            ...args
          }) {
            const sidebarItems = await defaultSidebarItemsGenerator(args)
            return reverseSidebarItems(
              excludeSidebarResults(sidebarItems, args.item),
              args.item
            )
          },
        },
        theme: {
          customCss: require.resolve("./src/css/custom.css"),
        },
        gtag: {
          trackingID: "G-S7G7X3JYS3",
        },
        sitemap: {
          filename: "sitemap-docs.xml",
        },
      },
    ],
  ],
  webpack: {
    jsLoader: (isServer) => ({
      loader: require.resolve("swc-loader"),
      options: {
        jsc: {
          parser: {
            syntax: "typescript",
            tsx: true,
          },
          target: "es2017",
        },
        module: {
          type: isServer ? "commonjs" : "es6",
        },
      },
    }),
  },
}

if (Object.keys(announcementBar).length) {
  config.themeConfig.announcementBar = announcementBar
}

export default config
