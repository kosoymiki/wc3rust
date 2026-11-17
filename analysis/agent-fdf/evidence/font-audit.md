# Font audit — operator WC3 UI data

- data: `/data/data/com.termux/files/home/src/herofolends-rs/analysis/war3_full3`
- FDF documents: **71**, string files: {'StartupStrings.txt': 2, 'MiscUI.txt': 90, 'TipStrings.txt': 62, 'HelpStrings.txt': 97, 'war3skins_labels': 43}
- font keys in `war3skins.txt`: 6; FDF keys used: ['EscMenuTextFont', 'InfoPanelTextFont', 'MasterFont']
- game font (MasterFont): `Fonts\FRIZQT__.TTF`
- unique chars in UI strings: **143**; Cyrillic used: **62**
- chars missing from the game font: **0**

## Font coverage

| font | bytes | ascii/84 | cyrillic/96 |
|---|---:|---:|---:|
| DFHeiMd.ttf | — | ERR | cmap format 4 subtable: glyph index arra |
| FRIZQT__.TTF | 143708 | 95 | 92 |
| NIM_____.ttf | 143708 | 95 | 92 |
| dffn_b31.ttf | — | ERR | cmap format 4 subtable: glyph index arra |
| dfst-m3u.ttf | 6221576 | 95 | 66 |
| tt5500m_.ttf | 5034348 | 95 | 66 |

## FDF font usage (top 15)

| key | size | frames |
|---|---:|---:|
| MasterFont | 0.011 | 36 |
| MasterFont | 0.013 | 17 |
| MasterFont | 0.015 | 12 |
| EscMenuTextFont | 0.011 | 5 |
| MasterFont | 0.009 | 3 |
| EscMenuTextFont | 0.013 | 3 |
| MasterFont | 0.024 | 2 |
| InfoPanelTextFont | 0.009 | 2 |
| EscMenuTextFont | 0.015 | 1 |
| InfoPanelTextFont | 0.013 | 1 |
| InfoPanelTextFont | 0.011 | 1 |

## Overflow candidates (width > 50% of 1920 at exact FDF size)

- none above threshold
