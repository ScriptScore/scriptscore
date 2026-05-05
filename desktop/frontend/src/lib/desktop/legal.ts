// SPDX-License-Identifier: AGPL-3.0-only
import type { LegalDisclosure } from '$lib/types';

import { invokeDesktopHostOrDefault } from './shared';

const FALLBACK_LEGAL_DISCLOSURE: LegalDisclosure = {
  licenseExpression: 'AGPL-3.0-only',
  sourceUrl: 'https://github.com/ScriptScore/scriptscore',
  localNoticesPath: 'desktop/dist/legal/THIRD_PARTY_NOTICES.md',
  thirdPartyNotices:
    'Generated third-party notices are bundled in packaged desktop builds. In dev and browser preview mode, run desktop/scripts/generate_legal_artifacts.py to refresh desktop/dist/legal/.',
  policyReportJson: '{}',
  artifactStatus: 'fallback'
};

export function getLegalDisclosure(): Promise<LegalDisclosure> {
  return invokeDesktopHostOrDefault<LegalDisclosure>(
    FALLBACK_LEGAL_DISCLOSURE,
    'get_legal_disclosure'
  ).catch(() => FALLBACK_LEGAL_DISCLOSURE);
}
