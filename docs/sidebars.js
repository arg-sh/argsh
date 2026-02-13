/**
 * Custom sidebar definitions:
 * - To declare a sidebar element as part of the homepage sidebar, add className: 'homepage-sidebar-item'
 * - To add an icon:
 *   - add the icon in www/docs/src/theme/Icon/<IconName>/index.ts as a React SVG element if it doesn't exist, where `<IconName>` is the camel case name of the icon
 *   - add the mapping to the icon in www/docs/src/theme/Icon/index.js
 *   - add in customProps sidebar_icon: 'icon-name'
 * - To add a group divider add in customProps sidebar_is_group_divider: true and set the label/value to the title that should appear in the divider.
 * - To add a back item, add in customProps:
 *   - sidebar_is_back_link: true
 *   - sidebar_icon: `back-arrow`
 * - To add a sidebar title, add in customProps sidebar_is_title: true
 * - To add a group headline, add in customProps sidebar_is_group_headline: true
 * - To add a coming soon link (with a badge), add in customProps sidebar_is_soon: true
 * - To add a badge, add in customProps sidebar_badge with its value being the props to pass to the Badge component.
 */

/** @type {import('@docusaurus/plugin-content-docs').SidebarsConfig} */
module.exports = {
  homepage: [
    {
      type: "doc",
      id: "homepage",
      label: "Overview",
      customProps: {
        sidebar_icon: "book-open",
      },
      className: "homepage-sidebar-item",
    },
    {
      type: "doc",
      id: "getting-started",
      label: "Getting Started",
      customProps: {
        sidebar_icon: "rocket-launch",
      },
      className: "homepage-sidebar-item",
    },
    {
      type: "category",
      label: "Styleguide",
      link: {
        type: "doc",
        id: "styleguide/index",
      },
      customProps: {
        sidebar_icon: "newspaper",
      },
      className: "homepage-sidebar-item",
      items: [
        {
          type: "doc",
          id: "styleguide/tldr",
          label: "tldr;",
          customProps: {
            iconName: "sparkles-solid",
            exclude_from_doc_list: true,
          },
        },
        {
          type: "doc",
          id: "styleguide/shell-files-and-interpreter-invocation",
          label: "Shell Files and Interpreter Invocation",
          customProps: {
            iconName: "document-text-solid",
          },
        },
        {
          type: "doc",
          id: "styleguide/environment",
          label: "Environment",
          customProps: {
            iconName: "circle-dotted-line",
          },
        },
        {
          type: "doc",
          id: "styleguide/comments",
          label: "Comments",
          customProps: {
            iconName: "chat-bubble-left-right-solid",
          },
        },
        {
          type: "doc",
          id: "styleguide/formatting",
          label: "Formatting",
          customProps: {
            iconName: "tools-solid",
          },
        },
        {
          type: "doc",
          id: "styleguide/features-and-bugs",
          label: "Features and Bugs",
          customProps: {
            iconName: "bug-ant-solid",
          },
        },
        {
          type: "doc",
          id: "styleguide/naming-conventions",
          label: "Naming Conventions",
          customProps: {
            iconName: "text",
          },
        },
        {
          type: "doc",
          id: "styleguide/calling-commands",
          label: "Calling Commands",
          customProps: {
            iconName: "channels-solid",
          },
        },
      ],
    },
    {
      type: "html",
      value: "Guides",
      customProps: {
        sidebar_is_group_divider: true,
      },
      className: "homepage-sidebar-item",
    },
    {
      type: "doc",
      id: "shell-completion",
      label: "Shell Completion",
      customProps: {
        sidebar_icon: "command-line",
      },
      className: "homepage-sidebar-item",
    },
    {
      type: "doc",
      id: "docgen",
      label: "Documentation Generation",
      customProps: {
        sidebar_icon: "document-text",
      },
      className: "homepage-sidebar-item",
    },
    {
      type: "category",
      label: "AI Integration",
      link: {
        type: "doc",
        id: "ai/index",
      },
      customProps: {
        sidebar_icon: "academic-cap-solid",
      },
      className: "homepage-sidebar-item",
      items: [
        {
          type: "doc",
          id: "ai/clis-for-llms",
          label: "CLIs for LLMs",
          customProps: {
            iconName: "document-text-solid",
          },
        },
        {
          type: "doc",
          id: "ai/mcp",
          label: "MCP Server",
          customProps: {
            iconName: "bolt-solid",
          },
        },
      ],
    },
    {
      type: "html",
      value: "Browse Docs",
      customProps: {
        sidebar_is_group_divider: true,
      },
      className: "homepage-sidebar-item",
    },
    {
      type: "ref",
      id: "development/overview",
      label: "Using argsh",
      customProps: {
        sidebar_icon: "server-stack",
      },
      className: "homepage-sidebar-item",
    },
    {
      type: "ref",
      id: "libraries/overview",
      label: "Libraries",
      customProps: {
        sidebar_icon: "book-open",
      },
      className: "homepage-sidebar-item",
    },
    {
      type: "html",
      value: "Core",
      customProps: {
        sidebar_is_group_divider: true,
      },
      className: "homepage-sidebar-item",
    },
    {
      type: "doc",
      id: "development/fundamentals/command-line-parser",
      label: "Command Line Parser",
      customProps: {
        sidebar_icon: "command-line",
      },
      className: "homepage-sidebar-item",
    },
    {
      type: "doc",
      id: "development/fundamentals/lint",
      label: "Lint",
      customProps: {
        sidebar_icon: "magnifying-glass",
      },
      className: "homepage-sidebar-item",
    },
    {
      type: "doc",
      id: "development/fundamentals/test",
      label: "Test",
      customProps: {
        sidebar_icon: "beaker",
      },
      className: "homepage-sidebar-item",
    },
    {
      type: "doc",
      id: "development/fundamentals/coverage",
      label: "Coverage",
      customProps: {
        sidebar_icon: "receipt-percent",
      },
      className: "homepage-sidebar-item",
    },
    {
      type: "doc",
      id: "development/fundamentals/docs",
      label: "Docs generation",
      customProps: {
        sidebar_icon: "academic-cap-solid",
      },
      className: "homepage-sidebar-item",
    },
    {
      type: "doc",
      id: "development/fundamentals/minify",
      label: "Minify",
      customProps: {
        sidebar_icon: "circle-dotted-line",
      },
      className: "homepage-sidebar-item",
    },
    {
      type: "doc",
      id: "development/fundamentals/builtins",
      label: "Native Builtins",
      customProps: {
        sidebar_icon: "bolt-solid",
      },
      className: "homepage-sidebar-item",
    },
    {
      type: "html",
      value: "Additional Resources",
      customProps: {
        sidebar_is_group_divider: true,
      },
      className: "homepage-sidebar-item",
    },
    {
      type: "doc",
      id: "troubleshooting",
      label: "Troubleshooting",
      customProps: {
        sidebar_icon: "bug",
      },
      className: "homepage-sidebar-item",
    },
    {
      type: "category",
      link: {
        type: "doc",
        id: "contribution/docs",
      },
      label: "Contribution Guidelines",
      customProps: {
        sidebar_icon: "document-text",
      },
      className: "homepage-sidebar-item",
      items: [
        {
          type: "autogenerated",
          dirName: "contribution",
        },
      ],
    },
    {
      type: "doc",
      id: "glossary",
      label: "Glossary",
      customProps: {
        sidebar_icon: "book-open",
      },
      className: "homepage-sidebar-item",
    },
    {
      type: "doc",
      id: "about",
      label: "About",
      customProps: {
        sidebar_icon: "light-bulb",
      },
      className: "homepage-sidebar-item",
    },
  ],
  libraries: [
    {
      type: "ref",
      id: "homepage",
      label: "Back to home",
      customProps: {
        sidebar_is_back_link: true,
        sidebar_icon: "back-arrow",
      },
    },
    {
      type: "doc",
      id: "libraries/overview",
      label: "Libraries",
      customProps: {
        sidebar_is_title: true,
        sidebar_icon: "book-open",
      },
    },
    {
      type: "category",
      label: "Argument Parsing",
      collapsible: false,
      customProps: {
        sidebar_is_group_headline: true,
      },
      items: [
        {
          type: "doc",
          id: "libraries/args",
          label: ":args",
          customProps: {
            sidebar_icon: "command-line-solid",
          },
          className: "homepage-sidebar-item",
        },
        {
          type: "doc",
          id: "libraries/args.utils",
          label: "args utils",
          customProps: {
            sidebar_icon: "command-line",
          },
          className: "homepage-sidebar-item",
        },
      ],
    },
    {
      type: "category",
      label: "Types",
      collapsible: false,
      customProps: {
        sidebar_is_group_headline: true,
      },
      items: [
        {
          type: "doc",
          id: "libraries/string",
          label: "string",
          customProps: {
            sidebar_icon: "text",
          },
          className: "homepage-sidebar-item",
        },
        {
          type: "doc",
          id: "libraries/array",
          label: "array",
          customProps: {
            sidebar_icon: "at-symbol",
          },
          className: "homepage-sidebar-item",
        },
        {
          type: "doc",
          id: "libraries/is",
          label: "is",
          customProps: {
            sidebar_icon: "magnifying-glass",
          },
          className: "homepage-sidebar-item",
        },
        {
          type: "doc",
          id: "libraries/to",
          label: "to",
          customProps: {
            sidebar_icon: "pencil-square-solid",
          },
          className: "homepage-sidebar-item",
        },
      ],
    },
    {
      type: "category",
      label: "Terminal",
      collapsible: false,
      customProps: {
        sidebar_is_group_headline: true,
      },
      items: [
        {
          type: "doc",
          id: "libraries/binary",
          label: "binary",
          customProps: {
            sidebar_icon: "play-solid",
          },
          className: "homepage-sidebar-item",
        },
        {
          type: "doc",
          id: "libraries/fmt",
          label: "fmt",
          customProps: {
            sidebar_icon: "document-text-solid",
          },
          className: "homepage-sidebar-item",
        },
      ],
    },
    {
      type: "category",
      label: "Utilities",
      collapsible: false,
      customProps: {
        sidebar_is_group_headline: true,
      },
      items: [
        {
          type: "doc",
          id: "libraries/error",
          label: "error",
          customProps: {
            sidebar_icon: "exclamation-circle-solid",
          },
          className: "homepage-sidebar-item",
        },
        {
          type: "doc",
          id: "libraries/bash",
          label: "bash",
          customProps: {
            sidebar_icon: "command-line",
          },
          className: "homepage-sidebar-item",
        },
        {
          type: "doc",
          id: "libraries/main",
          label: "main",
          customProps: {
            sidebar_icon: "play-solid",
          },
          className: "homepage-sidebar-item",
        },
      ],
    },
    {
      type: "category",
      label: "3rd Party",
      collapsible: false,
      customProps: {
        sidebar_is_group_headline: true,
      },
      items: [
        {
          type: "doc",
          id: "libraries/github",
          label: "github",
          customProps: {
            sidebar_icon: "github",
          },
          className: "homepage-sidebar-item",
        },
        {
          type: "doc",
          id: "libraries/docker",
          label: "docker",
          customProps: {
            sidebar_icon: "server-stack",
          },
          className: "homepage-sidebar-item",
        },
      ],
    },
  ],
  core: [
    {
      type: "ref",
      id: "homepage",
      label: "Back to home",
      customProps: {
        sidebar_is_back_link: true,
        sidebar_icon: "back-arrow",
      },
    },
    {
      type: "doc",
      id: "development/overview",
      label: "Development",
      customProps: {
        sidebar_is_title: true,
        sidebar_icon: "server-stack",
      },
    },
    {
      type: "category",
      label: "Core Concepts",
      collapsible: false,
      customProps: {
        sidebar_is_group_headline: true,
      },
      items: [
        {
          type: "doc",
          id: "development/fundamentals/command-line-parser",
          label: "Command Line Parser",
        },
        {
          type: "doc",
          id: "development/fundamentals/lint",
          label: "Lint",
        },
        {
          type: "doc",
          id: "development/fundamentals/test",
          label: "Test",
        },
        {
          type: "doc",
          id: "development/fundamentals/coverage",
          label: "Coverage",
        },
        {
          type: "doc",
          id: "development/fundamentals/docs",
          label: "Docs generation",
        },
        {
          type: "doc",
          id: "development/fundamentals/minify",
          label: "Minify",
        },
        {
          type: "doc",
          id: "development/fundamentals/builtins",
          label: "Native Builtins",
        },
      ],
    },
  ],
}
