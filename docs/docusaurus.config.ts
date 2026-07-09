import { themes as prismThemes } from 'prism-react-renderer';
import type { Config } from '@docusaurus/types';
import type * as Preset from '@docusaurus/preset-classic';

const config: Config = {
    title: 'SpecLance',
    tagline: 'Columnar, memory-mapped mass-spectrometry storage built on Lance',
    favicon: 'img/favicon.ico',

    markdown: {
        mermaid: true,
        hooks: {
            onBrokenMarkdownLinks: 'warn',
        },
    },
    plugins: ['docusaurus-plugin-llms-txt'],
    themes: ['@docusaurus/theme-mermaid'],

    url: 'https://sigilweaver.app',
    baseUrl: '/speclance/docs/',

    organizationName: 'Sigilweaver',
    projectName: 'SpecLance',

    onBrokenLinks: 'throw',

    i18n: {
        defaultLocale: 'en',
        locales: ['en'],
    },

    presets: [
        [
            'classic',
            {
                docs: {
                    routeBasePath: '/',
                    sidebarPath: './sidebars.ts',
                    editUrl: 'https://github.com/Sigilweaver/SpecLance/tree/main/docs/',
                },
                blog: false,
                sitemap: {
                    changefreq: 'weekly',
                    priority: 0.5,
                    filename: 'sitemap.xml',
                },
                theme: {
                    customCss: './src/css/custom.css',
                },
            } satisfies Preset.Options,
        ],
    ],

    themeConfig: {
        metadata: [
            { name: 'keywords', content: 'SpecLance, mass spectrometry, proteomics, Lance, Arrow, mzML, OpenProteo, columnar storage, Rust, Python' },
            { name: 'description', content: 'SpecLance is a columnar, memory-mapped mass-spectrometry store built on Lance. Ingests via OpenProteo or mzML; queryable from Rust and Python.' },
        ],
        colorMode: {
            defaultMode: 'dark',
            disableSwitch: false,
            respectPrefersColorScheme: true,
        },
        navbar: {
            title: 'Sigilweaver',
            logo: {
                alt: 'Sigilweaver logo',
                src: 'img/logo.svg',
                href: 'https://sigilweaver.app',
                target: '_self',
            },
            items: [
                {
                    label: 'OpenProteo',
                    href: 'https://sigilweaver.app/openproteo/docs/',
                    position: 'left',
                },
                {
                    label: 'Core',
                    href: 'https://docs.rs/speclance-core',
                    position: 'left',
                },
                {
                    href: 'https://github.com/Sigilweaver/SpecLance',
                    label: 'GitHub',
                    position: 'right',
                },
            ],
        },
        footer: {
            style: 'dark',
            links: [
                {
                    title: 'Project',
                    items: [
                        { label: 'GitHub', href: 'https://github.com/Sigilweaver/SpecLance' },
                        { label: 'Issues', href: 'https://github.com/Sigilweaver/SpecLance/issues' },
                    ],
                },
                {
                    title: 'Stack',
                    items: [
                        { label: 'OpenProteo', href: 'https://github.com/Sigilweaver/OpenProteo' },
                        { label: 'OpenProteoCore', href: 'https://github.com/Sigilweaver/OpenProteoCore' },
                        { label: 'Lance', href: 'https://lancedb.github.io/lance/' },
                    ],
                },
                {
                    title: 'Legal',
                    items: [
                        { label: 'Terms of Use', href: 'https://sigilweaver.app/terms' },
                        { label: 'Privacy Policy', href: 'https://sigilweaver.app/privacy' },
                    ],
                },
            ],
            copyright: `Copyright ${new Date().getFullYear()} Sigilweaver Holdings LLC. SpecLance is Apache-2.0 licensed. Documentation licensed under <a href="https://creativecommons.org/licenses/by-sa/4.0/" target="_blank" rel="noopener noreferrer">CC-BY-SA 4.0</a>.`,
        },
        prism: {
            theme: prismThemes.github,
            darkTheme: prismThemes.dracula,
            additionalLanguages: ['rust', 'toml', 'bash'],
        },
    } satisfies Preset.ThemeConfig,
};

export default config;
