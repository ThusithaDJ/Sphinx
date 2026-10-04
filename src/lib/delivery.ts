import type { SftpSite, TransportProtocol } from "./api";

/** What Sphinx knows about delivering files to a stock site over SFTP/FTPS. */
export interface DeliverySupport {
  /** False when the site doesn't take SFTP/FTPS uploads at all. */
  supported: boolean;
  /** Pre-filled for a site with no transport configured yet. */
  defaults?: { protocol: TransportProtocol; host: string; port: number; site: SftpSite };
  note: string;
}

const SITE_DELIVERY: Record<string, DeliverySupport> = {
  Shutterstock: {
    supported: true,
    defaults: { protocol: "ftps", host: "ftps.shutterstock.com", port: 21, site: "generic" },
    note: "Shutterstock accepts uploads over FTPS with your contributor login.",
  },
  "Adobe Stock": {
    supported: true,
    defaults: { protocol: "sftp", host: "sftp.contributor.adobestock.com", port: 22, site: "adobe_stock" },
    note: "Adobe Stock accepts uploads over SFTP with the SFTP password generated in the contributor portal.",
  },
  iStock: {
    supported: false,
    note: "iStock / Getty only accept uploads through the ESP web portal or app — use the Export CSV step for metadata.",
  },
};

const UNKNOWN_DELIVERY: DeliverySupport = {
  supported: true,
  note: "Enter the SFTP or FTPS details from this site's contributor docs. Leave it unconfigured if the site only takes web uploads.",
};

export function deliverySupportFor(siteName: string): DeliverySupport {
  return SITE_DELIVERY[siteName] ?? UNKNOWN_DELIVERY;
}
