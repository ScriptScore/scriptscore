// SPDX-License-Identifier: AGPL-3.0-only
import { ApproximatelyEqualIcon } from '@hugeicons/core-free-icons';
import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

import IconSelectField from './IconSelectField.svelte';
import SelectField from './SelectField.svelte';

describe('desktop overlay controls', () => {
  it('routes icon select interaction through shared popover behavior', async () => {
    const onChange = vi.fn();
    render(IconSelectField, {
      ariaLabel: 'Filter students',
      menuLabel: 'Filter',
      value: 'all',
      icon: ApproximatelyEqualIcon,
      options: [
        { value: 'all', label: 'All' },
        { value: 'ready', label: 'Ready', disabled: true },
        { value: 'uploaded', label: 'Uploaded' }
      ],
      onChange
    });

    const trigger = screen.getByRole('button', { name: 'Filter students' });
    await fireEvent.click(trigger);

    const dialog = screen.getByRole('dialog', { name: 'Filter students' });
    const disabledOption = within(dialog).getByRole('button', { name: 'Ready' });
    expect(disabledOption.hasAttribute('disabled')).toBe(true);

    await fireEvent.click(disabledOption);
    expect(onChange).not.toHaveBeenCalled();
    expect(screen.getByRole('dialog', { name: 'Filter students' })).toBeTruthy();

    await fireEvent.keyDown(document, { key: 'Escape' });
    expect(screen.queryByRole('dialog', { name: 'Filter students' })).toBeNull();
    expect(document.activeElement).toBe(trigger);

    await fireEvent.click(trigger);
    await fireEvent.click(within(screen.getByRole('dialog', { name: 'Filter students' })).getByRole('button', { name: 'Uploaded' }));
    expect(onChange).toHaveBeenCalledWith('uploaded');
    expect(screen.queryByRole('dialog', { name: 'Filter students' })).toBeNull();
  });

  it('routes custom select filtering and selection through shared popover behavior', async () => {
    const onChange = vi.fn();
    render(SelectField, {
      label: 'Course',
      value: 'math',
      searchable: true,
      allowEmpty: true,
      emptyLabel: 'No course',
      showSubitems: true,
      options: [
        { value: 'math', label: 'Math', subitem: 'Period 1' },
        { value: 'science', label: 'Science', subitem: 'Period 2', disabled: true },
        { value: 'history', label: 'History', subitem: 'Period 3' }
      ],
      onChange
    });

    const trigger = screen.getByRole('combobox', { name: 'Course: Math' });
    await fireEvent.click(trigger);

    await fireEvent.input(screen.getByRole('textbox', { name: 'Search' }), {
      target: { value: 'science' }
    });

    const listbox = screen.getByRole('listbox', { name: 'Course' });
    expect(within(listbox).queryByRole('option', { name: /Math/ })).toBeNull();
    const disabledOption = within(listbox).getByRole('option', { name: /Science/ });
    expect(disabledOption.hasAttribute('disabled')).toBe(true);

    await fireEvent.click(disabledOption);
    expect(onChange).not.toHaveBeenCalled();
    expect(screen.getByRole('listbox', { name: 'Course' })).toBeTruthy();

    await fireEvent.keyDown(document, { key: 'Escape' });
    expect(screen.queryByRole('listbox', { name: 'Course' })).toBeNull();
    expect(document.activeElement).toBe(trigger);

    await fireEvent.click(trigger);
    await fireEvent.input(screen.getByRole('textbox', { name: 'Search' }), {
      target: { value: 'history' }
    });
    await fireEvent.click(screen.getByRole('option', { name: /History/ }));
    expect(onChange).toHaveBeenCalledWith('history');
    expect(screen.queryByRole('listbox', { name: 'Course' })).toBeNull();
  });

  it('closes custom select popovers on outside pointer interaction', async () => {
    render(SelectField, {
      label: 'Course',
      value: 'math',
      options: [
        { value: 'math', label: 'Math' },
        { value: 'science', label: 'Science' }
      ]
    });

    await fireEvent.click(screen.getByRole('combobox', { name: 'Course: Math' }));
    expect(screen.getByRole('listbox', { name: 'Course' })).toBeTruthy();

    await fireEvent.pointerDown(document.body);

    expect(screen.queryByRole('listbox', { name: 'Course' })).toBeNull();
  });

  it('supports multiselect custom select options without closing the popover', async () => {
    const onChangeValues = vi.fn();
    render(SelectField, {
      ariaLabel: 'Command filter',
      multiple: true,
      values: ['exam.setup'],
      allowEmpty: true,
      emptyLabel: 'All commands',
      options: [
        { value: 'exam.setup', label: 'exam.setup' },
        { value: 'exam.analyze', label: 'exam.analyze' }
      ],
      onChangeValues
    });

    await fireEvent.click(screen.getByRole('combobox', { name: 'Command filter' }));
    await fireEvent.click(screen.getByRole('option', { name: 'exam.analyze' }));

    expect(onChangeValues).toHaveBeenCalledWith(['exam.setup', 'exam.analyze']);
    expect(screen.getByRole('listbox', { name: 'Command filter' })).toBeTruthy();

    await fireEvent.click(screen.getByRole('option', { name: 'All commands' }));
    expect(onChangeValues).toHaveBeenLastCalledWith([]);
  });

  it('targets generated trigger ids from labels instead of wrapping custom popovers', () => {
    render(SelectField, {
      label: 'Course',
      value: 'math',
      options: [
        { value: 'math', label: 'Math' },
        { value: 'science', label: 'Science' }
      ]
    });

    const label = screen.getByText('Course');
    const trigger = screen.getByRole('combobox', { name: 'Course: Math' });

    expect(label.tagName).toBe('LABEL');
    expect(label.getAttribute('for')).toBe(trigger.getAttribute('id'));
    expect(label.contains(trigger)).toBe(false);
  });
});
