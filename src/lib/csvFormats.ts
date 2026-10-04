import type { GeneratedMetadata } from "./api";

/** One row's worth of data available to a CSV column. */
export interface CsvRow {
  fileName: string;
  metadata: GeneratedMetadata;
}

/** What fills a column. "blank" is site-specific data Sphinx doesn't track
 * (categories, releases, pricing...) -- the site fills a default or the user
 * completes it in a spreadsheet before uploading. */
export type CsvSource = "filename" | "title" | "description" | "keywords" | "blank";

export const CSV_SOURCES: { value: CsvSource; label: string }[] = [
  { value: "filename", label: "File name" },
  { value: "title", label: "Title" },
  { value: "description", label: "Description" },
  { value: "keywords", label: "Keywords" },
  { value: "blank", label: "Blank" },
];

export interface CsvColumn {
  header: string;
  source: CsvSource;
}

export interface CsvFormat {
  columns: CsvColumn[];
  /** How the keyword column joins keywords. */
  keywordSeparator: string;
  note?: string;
  /** True when `columns` comes from a user-edited layout rather than the
   * built-in template. */
  customized: boolean;
}

/** User-edited column layouts, keyed by site name. Persisted per project. */
export type CsvLayouts = Record<string, CsvColumn[]>;

type Template = Omit<CsvFormat, "customized">;

const col = (header: string, source: CsvSource = "blank"): CsvColumn => ({ header, source });

/** Bulk-metadata CSV layouts per built-in site, based on each site's
 * documented upload template at the time of writing. Like the limiter
 * presets, these are a starting point rather than a guarantee -- users can
 * edit them per site in Sites. */
const SITE_FORMATS: Record<string, Template> = {
  Shutterstock: {
    keywordSeparator: ",",
    columns: [
      col("Filename", "filename"),
      col("Description", "title"),
      col("Keywords", "keywords"),
      col("Categories"),
      col("Editorial"),
      col("Mature content"),
      col("illustration"),
    ],
    note: "Shutterstock has no separate title — the title goes in Description.",
  },
  "Adobe Stock": {
    keywordSeparator: ",",
    columns: [col("Filename", "filename"), col("Title", "title"), col("Keywords", "keywords"), col("Category"), col("Releases")],
  },
  iStock: {
    keywordSeparator: ",",
    columns: [
      col("file name", "filename"),
      col("created date"),
      col("description", "description"),
      col("country"),
      col("brief code"),
      col("title", "title"),
      col("keywords", "keywords"),
    ],
  },
  Dreamstime: {
    keywordSeparator: ",",
    columns: [
      col("Filename", "filename"),
      col("Image Name", "title"),
      col("Description", "description"),
      col("Category 1"),
      col("Category 2"),
      col("Category 3"),
      col("keywords", "keywords"),
      col("Free"),
      col("W-EL"),
      col("P-EL"),
      col("SR-EL"),
      col("SR-Price"),
      col("Editorial"),
      col("MR doc Ids"),
      col("Pr Docs"),
    ],
  },
  "123RF": {
    keywordSeparator: ",",
    columns: [
      col("oldfilename", "filename"),
      col("123rf_filename"),
      col("description", "title"),
      col("keywords", "keywords"),
      col("country"),
    ],
  },
  Pond5: {
    keywordSeparator: ",",
    columns: [
      col("originalfilename", "filename"),
      col("title", "title"),
      col("description", "description"),
      col("keywords", "keywords"),
      col("location"),
      col("price"),
    ],
  },
};

/** Used for custom site profiles that have no known template. */
const GENERIC_FORMAT: Template = {
  keywordSeparator: "; ",
  columns: [col("Filename", "filename"), col("Title", "title"), col("Description", "description"), col("Keywords", "keywords")],
  note: "Generic layout — custom sites have no known template.",
};

/** The built-in template for `siteName`, ignoring any user edits. */
export function defaultCsvColumns(siteName: string): CsvColumn[] {
  return (SITE_FORMATS[siteName] ?? GENERIC_FORMAT).columns.map((c) => ({ ...c }));
}

export function csvFormatFor(siteName: string, layouts: CsvLayouts = {}): CsvFormat {
  const template = SITE_FORMATS[siteName] ?? GENERIC_FORMAT;
  const custom = layouts[siteName];
  return custom ? { ...template, columns: custom, customized: true } : { ...template, customized: false };
}

/** Strips leading `= + - @`, tab and CR so a spreadsheet never evaluates the
 * cell as a formula. Not applied to filenames, which must match the file. */
function textCell(s: string): string {
  return s.replace(/^[=+\-@\t\r ]+/, "");
}

function cellValue(source: CsvSource, row: CsvRow, keywordSeparator: string): string {
  switch (source) {
    case "filename":
      return row.fileName;
    case "title":
      return textCell(row.metadata.title);
    case "description":
      return textCell(row.metadata.description || row.metadata.title);
    case "keywords":
      return textCell(row.metadata.keywords.join(keywordSeparator));
    case "blank":
      return "";
  }
}

/** RFC 4180 quoting for fields containing a comma, quote, or newline. */
function csvField(s: string): string {
  return /[",\r\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
}

export function buildCsv(format: CsvFormat, rows: CsvRow[]): string {
  const { columns, keywordSeparator } = format;
  const lines = [columns.map((c) => csvField(c.header)).join(",")];
  for (const row of rows) {
    lines.push(columns.map((c) => csvField(cellValue(c.source, row, keywordSeparator))).join(","));
  }
  return lines.join("\r\n") + "\r\n";
}
