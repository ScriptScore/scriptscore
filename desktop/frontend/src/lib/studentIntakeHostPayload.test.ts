// SPDX-License-Identifier: AGPL-3.0-only
import { describe, expect, it } from 'vitest';

import {
  sanitizeStudentIntakeInputForHost,
  sanitizeStudentIntakeInputsForHost
} from './studentIntakeHostPayload';

describe('sanitizeStudentIntakeInputForHost', () => {
  it('rounds fractional pixel geometry to integers for the Tauri host', () => {
    const input = {
      studentRef: 'student_1',
      localStudentName: ' Ada Local ',
      rawPdfPath: '/tmp/exam.pdf',
      // Near-integer float noise (e.g. from array round-trips), not semantic page indices.
      desiredPageOrder: [2 + 1e-10, 1 + 1e-10],
      redactionRegionsPx: [
        {
          pageNumber: 1,
          x: 100.4,
          y: 200.6,
          width: 619.1999999999999,
          height: 400.2
        }
      ],
      rasterSizesByPage: {
        1: { widthPx: 600.7, heightPx: 800.3 }
      }
    };
    const out = sanitizeStudentIntakeInputForHost(input);
    expect(out.localStudentName).toBe('Ada Local');
    expect(out.desiredPageOrder).toEqual([2, 1]);
    expect(out.redactionRegionsPx).toEqual([
      {
        pageNumber: 1,
        x: 100,
        y: 201,
        width: 619,
        height: 400
      }
    ]);
    expect(out.rasterSizesByPage).toEqual({
      1: { widthPx: 601, heightPx: 800 }
    });
  });

  it('maps multiple inputs', () => {
    const [a] = sanitizeStudentIntakeInputsForHost([
      {
        studentRef: 'a',
        rawPdfPath: '/a.pdf',
        redactionRegionsPx: [{ pageNumber: 1, x: 0.5, y: 0.5, width: 10.5, height: 10.5 }]
      }
    ]);
    expect(a.redactionRegionsPx?.[0]?.x).toBe(1);
  });
});
