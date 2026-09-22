export type LiteratureFileInfo = {
  name: string;
  size: number;
  updated_at?: number;
  document_id?: string;
};

export type LiteraturePreviewBlock = {
  content: string;
};

export type LiteratureFilePreview = {
  folder: string;
  filename: string;
  processed: boolean;
  source?: string;
  content?: string;
  blocks: LiteraturePreviewBlock[];
  raw_data_url?: string;
  raw_sheets?: { name: string; html: string }[];
  asset_data_urls?: Record<string, string>;
};
