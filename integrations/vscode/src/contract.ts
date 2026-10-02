export const SUPPORTED_CLI_API_VERSION = 1;
export const SUPPORTED_JSON_SCHEMA_VERSION = 1;
export const SUPPORTED_PROGRESS_SCHEMA_VERSION = 1;

export interface VersionPayload {
  version: string;
  cli_api_version: number;
  json_schema_version: number;
  progress_schema_version: number;
}

export interface CapabilitiesPayload extends VersionPayload {
  features: Record<string, boolean>;
}

export interface Profile {
  name: string;
  auth?: string;
  label?: string | null;
}

export interface ProfileList {
  [tool: string]: { active: string | null; profiles: Profile[] };
}

export interface ContextEntry {
  name: string;
  profiles: Record<string, string | null>;
}

export interface ContextList {
  contexts: ContextEntry[];
}

export interface StatusPayload {
  tools: Array<{ tool: string; active_profile: string | null }>;
  context: {
    status: string;
    active: string | null;
    profiles: Record<string, string | null> | null;
  };
}

export interface JsonEnvelope {
  ok: boolean;
  command: string;
  result?: unknown;
  error?: { kind?: string; message?: string; remediation?: string[] };
}

export function assertCompatible(payload: VersionPayload): void {
  if (payload.cli_api_version !== SUPPORTED_CLI_API_VERSION) {
    throw new Error(
      `Unsupported aisw CLI API version ${payload.cli_api_version}; ` +
        `this extension requires ${SUPPORTED_CLI_API_VERSION}. Upgrade aisw.`,
    );
  }
  if (payload.json_schema_version !== SUPPORTED_JSON_SCHEMA_VERSION) {
    throw new Error(
      `Unsupported aisw JSON schema version ${payload.json_schema_version}; ` +
        `this extension requires ${SUPPORTED_JSON_SCHEMA_VERSION}. Upgrade aisw.`,
    );
  }
  if (payload.progress_schema_version !== SUPPORTED_PROGRESS_SCHEMA_VERSION) {
    throw new Error(
      `Unsupported aisw progress schema version ${payload.progress_schema_version}; ` +
        `this extension requires ${SUPPORTED_PROGRESS_SCHEMA_VERSION}. Upgrade aisw.`,
    );
  }
}

export function assertFeatures(
  capabilities: CapabilitiesPayload,
  required: string[] = ['mutation_json', 'verify', 'contexts'],
): void {
  const missing = required.filter((feature) => capabilities.features[feature] !== true);
  if (missing.length > 0) {
    throw new Error(`Installed aisw CLI is missing required features: ${missing.join(', ')}`);
  }
}
