import { supabaseApplicationEnvironment } from './supabase-lifecycle.js';
import { deployStartScriptReference, startScriptLayout, type ReferenceDeployInput } from '../stack-reference-operations.js';

export const SUPABASE_REFERENCE_LAYOUT = startScriptLayout('supabase');

export const deploySupabaseReference = async (input: ReferenceDeployInput): Promise<void> =>
  deployStartScriptReference(input, supabaseApplicationEnvironment(input.lease), 'Supabase');
