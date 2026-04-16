import {themes as prismThemes} from 'prism-react-renderer';

/** @type {import('@docusaurus/types').Config} */
const config = {
  title: 'valsb',
  tagline: 'A lightweight CLI tool to manage sing-box proxy across all desktop platforms',
  favicon: 'img/favicon.ico',

  future: {
    v4: true,
  },

  url: 'https://nsevo.github.io',
  baseUrl: '/val-sing-box-cli/',
  trailingSlash: true,

  organizationName: 'nsevo',
  projectName: 'val-sing-box-cli',

  onBrokenLinks: 'throw',

  i18n: {
    defaultLocale: 'en',
    locales: ['en', 'zh-Hans'],
    localeConfigs: {
      en: { label: 'English' },
      'zh-Hans': { label: '简体中文' },
    },
  },

  presets: [
    [
      'classic',
      /** @type {import('@docusaurus/preset-classic').Options} */
      ({
        docs: {
          routeBasePath: '/',
          sidebarPath: './sidebars.js',
          editUrl: 'https://github.com/nsevo/val-sing-box-cli/tree/main/website/',
        },
        blog: false,
        pages: false,
        theme: {
          customCss: './src/css/custom.css',
        },
      }),
    ],
  ],

  themeConfig:
    /** @type {import('@docusaurus/preset-classic').ThemeConfig} */
    ({
      colorMode: {
        defaultMode: 'dark',
        respectPrefersColorScheme: true,
      },
      navbar: {
        title: 'valsb',
        hideOnScroll: true,
        items: [
          {
            type: 'localeDropdown',
            position: 'right',
          },
          {
            href: 'https://github.com/nsevo/val-sing-box-cli',
            label: 'GitHub',
            position: 'right',
          },
        ],
      },
      prism: {
        theme: prismThemes.github,
        darkTheme: prismThemes.dracula,
        additionalLanguages: ['bash', 'powershell', 'json', 'toml'],
      },
    }),
};

export default config;
