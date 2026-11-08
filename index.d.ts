export interface CompiledProfile {
  preset: string;
  program: {rules: unknown[]};
  exclusions: string[];
}
export function profiles(): string[];
export function compileProfile(selector: string): CompiledProfile;
export function version(): string;
