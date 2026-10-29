# Roadmap

HexaGraph aims at feature parity with the commercial scientific graphing
tools that biologists use every day, built clean-room: the specification is the
**publicly available documentation** of those tools and the statistical
literature. We never decompile, copy or translate proprietary code, text or
artwork.

Legend: ✅ done · 🟡 partial · ⬜ to do

## 1. Data tables

| Feature | Status |
|---|---|
| XY, Column, Grouped, Contingency, Survival, Parts of whole tables | ✅ |
| Unbounded spreadsheet: arrows, Tab, Enter, block selection, cut/copy/paste, paste transposed, insert/delete rows, CSV import/export | ✅ |
| Sheet grows as you type (new data sets, rows) | ✅ |
| Replicates in side-by-side subcolumns | ✅ |
| *Nested* table (subcolumns are nested groups) | ⬜ |
| *Multiple variables* table (one row per subject, many variables) | ⬜ |
| Y entry as Mean/SD/N, Mean/SEM/N, Mean ± error (instead of raw replicates) | ⬜ |
| X as dates or elapsed times; X error bars | ⬜ |
| Exclude values from an analysis (outlier check in the Analyze dialog) | ✅ |
| Exclude values from every analysis and graph (struck through in the sheet, Ctrl+E) | ✅ |
| Column formatting: decimals, number format | ⬜ |
| Info sheets and notes with rich text | ⬜ |
| Import Prism XML projects (.pzfx): data tables | ✅ |
| Import Prism 10 projects (.prism): tables, excluded values, analyses (recalculated and compared with Prism's results) | ✅ |
| Import Excel files (`calamine`) | ⬜ |

## 2. Data processing

| Feature | Status |
|---|---|
| Transform (log, ln, reciprocal, power, user formula), X and Y | ⬜ |
| Normalize (0–100%, by first/last value, by sum) | ⬜ |
| Remove baseline, subtract column/row | ⬜ |
| Transpose, prune, select rows | ⬜ |
| Fraction of total | ✅ |
| Frequency distribution, cumulative distribution | ⬜ |
| Chained analyses (analyze the results of an analysis) | ⬜ |

## 3. Statistics

| Feature | Status |
|---|---|
| Descriptive statistics, normality tests (4), one-sample t and Wilcoxon, Grubbs | ✅ |
| Data check before a test: outliers, normality, equal SDs, suggested test | ✅ |
| Any test on any table, with a warning when the data are adapted | ✅ |
| ROUT outlier method | ⬜ |
| t tests (unpaired, Welch, paired), Mann-Whitney, Wilcoxon, KS — one/two tails, CI level | ✅ |
| One-way ANOVA (ordinary, Welch, RM), Kruskal-Wallis, Friedman | ✅ |
| Multiple comparisons: Tukey, Bonferroni, Šídák, Holm-Šídák, LSD, Dunn; vs. control | ✅ |
| Dunnett's test (multivariate t) | ✅ |
| Games-Howell after Welch's ANOVA | ✅ |
| Dunnett T3 | ⬜ |
| Two-way ANOVA (Type III, unbalanced), comparisons within rows/columns | ✅ |
| Two-way RM ANOVA, mixed-effects model, three-way ANOVA | ⬜ |
| Nested t test, nested one-way ANOVA | ⬜ |
| Multiple t tests (one per row) with FDR or Holm-Šídák | ✅ |
| Correlation (Pearson, Spearman), simple linear regression | ✅ |
| Correlation matrix, multiple linear regression, logistic regression, PCA | ⬜ |
| Nonlinear regression: 17 models, constraints | ✅ |
| Shared parameters, model comparison (F test, AICc), weighting, robust fit | ⬜ |
| Interpolate unknowns from a standard curve; prediction bands (confidence band of linear regression ✅) | ⬜ |
| User-defined equations | ⬜ |
| Kaplan-Meier, log-rank, Gehan-Breslow-Wilcoxon, hazard ratio | ✅ |
| Log-rank test for trend | ✅ |
| Cox proportional hazards | ⬜ |
| Chi-square, Fisher, relative risk, odds ratio, trend | ✅ |
| Bland-Altman, ROC | ✅ |
| Power and sample size | ⬜ |

## 4. Graphs

| Feature | Status |
|---|---|
| Column graphs: scatter dot, bar, box, violin, mean ± error, before-after | ✅ |
| Grouped/stacked bars and scatter, by row or by column | ✅ |
| XY with fitted curves, Kaplan-Meier, pie and donut | ✅ |
| Significance brackets, live from a test or by hand | ✅ |
| Custom colors: disc, classic, harmony, RGB/HEX/CMYK, palettes | ✅ |
| Interactive canvas: zoom, pan, click to select, drag labels, legend and brackets, resize the page | ✅ |
| Symbols (7 shapes, filled/hollow) per data set, bar opacity/width, per-text font sizes | ✅ |
| User palettes saved across sessions | ✅ |
| Histogram, QQ plot, line of means, interaction plot, grouped box plots, heat map | ✅ |
| Graph recommendation from the data | ✅ |
| Bubble, forest, residual, Bland-Altman and ROC graphs, estimation plots | ⬜ |
| Rotated axis labels, error bars above only, separate line thickness per part | ✅ |
| Axis breaks, custom ticks, right Y axis | ⬜ |
| Drawing tools: text, arrows, lines, shapes, images on the graph | ⬜ |
| Graph styles / themes, "magic" apply-style-to-many | ⬜ |
| Layouts: several graphs and tables on a page | ⬜ |

## 5. Output

| Feature | Status |
|---|---|
| PNG, JPEG, TIFF (any dpi), SVG | ✅ |
| PDF and EPS | ⬜ |
| Copy graph to the clipboard; print | ⬜ |
| Results to the clipboard (Excel/Word) | ✅ |
| Report export (all results of a project as HTML/PDF) | ⬜ |

## 6. Application

| Feature | Status |
|---|---|
| Light and dark theme, Inter font, Lucide icons | ✅ |
| Undo/redo, save prompt, project file (JSON) | ✅ |
| Command-line tool (render graphs / run analyses headless) | ⬜ |
| MCP server so AI agents can drive the app | ⬜ |
| Web version (WebAssembly) | ⬜ |
| Installers for Windows, macOS, Linux (Flatpak) | ⬜ |
| Translations (Italian first) | ⬜ |
