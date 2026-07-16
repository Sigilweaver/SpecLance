import type { SidebarsConfig } from '@docusaurus/plugin-content-docs';

const sidebars: SidebarsConfig = {
    docsSidebar: [
        'intro',
        {
            type: 'category',
            label: 'Guide',
            collapsed: false,
            items: [
                'guide/python-api',
            ],
        },
    ],
};

export default sidebars;
