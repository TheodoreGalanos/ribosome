import { RpcError } from '../client/validation.js';

/** Select only the configured provider's environment. Never include these
 * values in prompts, checkpoints, host-tool environments or diagnostics. */
export function providerEnvironment(provider: string, env: NodeJS.ProcessEnv = process.env): Record<string, string> {
  let names: string[];
  switch (provider) {
    case 'openai': names = ['OPENAI_API_KEY']; break;
    case 'anthropic': names = ['ANTHROPIC_API_KEY']; break;
    case 'azure-openai-responses': names = [
      'AZURE_OPENAI_API_KEY', 'AZURE_OPENAI_BASE_URL', 'AZURE_OPENAI_RESOURCE_NAME',
      'AZURE_OPENAI_API_VERSION', 'AZURE_OPENAI_DEPLOYMENT_NAME_MAP',
    ]; break;
    default: throw new RpcError(-32602, 'supported providers are openai, anthropic and azure-openai-responses');
  }
  const selected = Object.fromEntries(names.flatMap(name => env[name]?.trim() ? [[name, env[name]!]] : []));
  if (!selected[names[0]!]) throw new RpcError(-32001, `${names[0]} is not configured`);
  if (provider === 'azure-openai-responses') {
    if (!selected.AZURE_OPENAI_BASE_URL && !selected.AZURE_OPENAI_RESOURCE_NAME) {
      throw new RpcError(-32602, 'Configure AZURE_OPENAI_BASE_URL or AZURE_OPENAI_RESOURCE_NAME');
    }
    if (selected.AZURE_OPENAI_BASE_URL) {
      let url;
      try { url = new URL(selected.AZURE_OPENAI_BASE_URL); } catch { throw new RpcError(-32602, 'AZURE_OPENAI_BASE_URL must be an absolute URL'); }
      if (url.protocol !== 'https:' || url.username || url.password || url.search || url.hash || url.pathname.includes('/deployments/')) {
        throw new RpcError(-32602, 'AZURE_OPENAI_BASE_URL must be an HTTPS resource or /openai/v1 URL without credentials, query parameters or a deployment path');
      }
    }
    if (selected.AZURE_OPENAI_RESOURCE_NAME && !/^[a-zA-Z0-9-]+$/.test(selected.AZURE_OPENAI_RESOURCE_NAME)) {
      throw new RpcError(-32602, 'AZURE_OPENAI_RESOURCE_NAME must contain only letters, digits and hyphens');
    }
    const mappings = selected.AZURE_OPENAI_DEPLOYMENT_NAME_MAP;
    if (mappings) {
      const seen = new Set<string>();
      for (const entry of mappings.split(',')) {
        if (!/^\s*[^\s=,]+\s*=\s*[^\s=,]+\s*$/.test(entry)) throw new RpcError(-32602, 'AZURE_OPENAI_DEPLOYMENT_NAME_MAP requires comma-separated model-id=deployment pairs');
        const model = entry.split('=')[0]!.trim();
        if (seen.has(model)) throw new RpcError(-32602, 'AZURE_OPENAI_DEPLOYMENT_NAME_MAP contains duplicate model IDs');
        seen.add(model);
      }
    }
  }
  return selected;
}
