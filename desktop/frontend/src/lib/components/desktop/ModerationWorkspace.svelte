<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import { HugeiconsIcon } from '@hugeicons/svelte';
  import {
    AccountSetting01Icon,
    Add01Icon,
    Cancel01Icon,
    CheckListIcon,
    EyeIcon,
    File02Icon,
    FileQuestionMarkIcon,
    SquareArrowShrink02Icon
  } from '@hugeicons/core-free-icons';
  import {
    highlightKindToClass,
    markedTextSegments
  } from '$lib/components/desktop/student-workflow-helpers';
  import { toDesktopAssetUrl } from '$lib/desktop/shared';
  import type { ExamWorkspaceState, StudentWorkflowAnswer } from '$lib/types';
  import {
    DesktopPopover,
    IconButton,
    IconSelectField,
    PagePreviewFrame,
    SegmentedControl,
    StatusBadge,
    ToggleRow,
    compactTabActionButtonClass,
    type FeedbackTone
  } from './ui';

  type EvidenceViewMode = 'png' | 'text' | 'both';

  type ModerationCard = {
    studentRef: string;
    answer: StudentWorkflowAnswer;
    originalTotalPoints: number;
    effectiveTotalPoints: number;
    effectiveFeedbackText: string;
    hasScoreOverride: boolean;
    hasFeedbackOverride: boolean;
    pageImagePath: string | null;
    pageNumber: number | null;
  };

  type ModerationQuestionGroup = {
    questionId: string;
    questionNumber: number;
    promptText: string;
    maxPoints: number;
    reviewed: boolean;
    cards: ModerationCard[];
  };

  type CardPreview =
    | { kind: 'page'; cardKey: string; card: ModerationCard }
    | { kind: 'rubric'; cardKey: string; card: ModerationCard };

  type CardPointerDragState = {
    cardKey: string;
    pointerId: number;
    startX: number;
    startY: number;
    latestX: number;
    latestY: number;
    dragging: boolean;
  };

  export let workspaceState: ExamWorkspaceState;
  export let studentDisplayNamesByRef: Record<string, string> = {};
  export let busy = false;
  export let onSaveModeratedScore:
    | ((studentRef: string, questionId: string, moderatedTotalPoints: number) => Promise<void>)
    | null = null;
  export let onSaveModeratedFeedback:
    | ((studentRef: string, questionId: string, feedbackText: string) => Promise<void>)
    | null = null;
  export let onSetQuestionReviewed:
    | ((questionId: string, reviewed: boolean) => Promise<boolean>)
    | null = null;

  const tabActionButtonClass = compactTabActionButtonClass;
  const evidenceViewOptions = [
    { value: 'png', label: 'PNG' },
    { value: 'text', label: 'Text' },
    { value: 'both', label: 'Both' }
  ];

  let selectedQuestionId: string | null = null;
  let evidenceView: EvidenceViewMode = 'both';
  let showStudentNames = false;
  let pendingCardKey: string | null = null;
  let pendingFeedbackKey: string | null = null;
  let pendingReviewQuestionId: string | null = null;
  let draggingCardKey: string | null = null;
  let hoverLaneKey: string | null = null;
  let feedbackDrafts: Record<string, string> = {};
  let visibleLanesByQuestion: Record<string, number[]> = {};
  let optimisticScores: Partial<Record<string, number>> = {};
  let cardSize = 32;
  let evidenceControlsOpen = false;
  let compactCardsByKey: Record<string, boolean> = {};
  let activePreview: CardPreview | null = null;
  let activePreviewStyle = '';
  let moderationQuestions: ModerationQuestionGroup[] = [];
  let selectedQuestion: ModerationQuestionGroup | null = null;
  let selectedQuestionCards: ModerationCard[] = [];
  let scoreLanes: number[] = [];
  let pointerDragState: CardPointerDragState | null = null;
  let lanesRenderKey = 0;
  let cardColumnWidthStyle = '15.2rem';
  let cardMinHeightStyle = '11rem';
  let textBlockMaxHeightStyle = '5.777777777777778rem';
  let imageBlockMaxHeightStyle = '6.5rem';
  let feedbackBlockMaxHeightStyle = '3.5rem';
  let evidenceFontSizeStyle = '0.6875rem';
  let evidenceLineHeightStyle = '1rem';
  let feedbackFontSizeStyle = '0.6875rem';
  let feedbackLineHeightStyle = '1rem';
  let moderationTipsOpen = false;
  let previewPopoverRef: HTMLDivElement | null = null;
  let previewTriggerElement: HTMLElement | null = null;
  let previewOpenSuppressedTarget: Node | null = null;
  let previewOpenSuppressionTimer: number | null = null;

  function cardKey(studentRef: string, questionId: string): string {
    return `${studentRef}:${questionId}`;
  }

  function laneKey(questionId: string, score: number): string {
    return `${questionId}:${score}`;
  }

  function scoreLaneDatasetValue(questionId: string, score: number): string {
    return `${questionId}:${score}`;
  }

  function questionPromptText(questionId: string): string {
    const question = workspaceState.questions.find((item) => item.questionId === questionId);
    return question?.text?.trim() || `Question ${questionId}`;
  }

  function questionNumber(questionId: string): number {
    const question = workspaceState.questions.find((item) => item.questionId === questionId);
    return question?.questionNumber ?? 0;
  }

  function questionPageNumber(questionId: string): number | null {
    const question = workspaceState.questions.find((item) => item.questionId === questionId);
    return question?.pageNumber ?? null;
  }

  function questionMaxPoints(questionId: string, cards: ModerationCard[]): number {
    const question = workspaceState.questions.find((item) => item.questionId === questionId);
    if (question?.maxPoints !== null && question?.maxPoints !== undefined) {
      return question.maxPoints;
    }
    return Math.max(
      0,
      ...cards.map((card) => card.answer.questionMaxPoints ?? card.originalTotalPoints ?? 0)
    );
  }

  function reviewLookup(questionId: string): boolean {
    return (
      workspaceState.moderationState?.questionReviews?.some(
        (item) => item.questionId === questionId
      ) ?? false
    );
  }

  function effectiveFeedback(studentRef: string, answer: StudentWorkflowAnswer): {
    effectiveFeedbackText: string;
    hasOverride: boolean;
  } {
    const override =
      workspaceState.moderationState?.feedbackOverrides?.find(
        (item) => item.studentRef === studentRef && item.questionId === answer.questionId
      ) ?? null;
    return {
      effectiveFeedbackText: override?.feedbackText ?? answer.feedbackText ?? '',
      hasOverride: override !== null
    };
  }

  $: moderationQuestions = (() => {
    const grouped = new Map<string, ModerationCard[]>();
    for (const submission of workspaceState.studentWorkflow?.submissions ?? []) {
      for (const answer of submission.answers ?? []) {
        if (!answer.moderationEligible) {
          continue;
        }
        const optimisticScore =
          optimisticScores[cardKey(submission.studentRef, answer.questionId)];
        const persistedOverride =
          workspaceState.moderationState?.scoreOverrides?.find(
            (item) =>
              item.studentRef === submission.studentRef && item.questionId === answer.questionId
          ) ?? null;
        const score = {
          effectiveTotalPoints:
            optimisticScore ??
            persistedOverride?.moderatedTotalPoints ??
            answer.totalPointsAwarded ??
            0,
          hasOverride:
            optimisticScore !== undefined
              ? optimisticScore !== (answer.totalPointsAwarded ?? 0)
              : persistedOverride !== null
        };
        const feedback = effectiveFeedback(submission.studentRef, answer);
        const answerPageNumber = questionPageNumber(answer.questionId);
        const answerPage =
          answerPageNumber === null
            ? null
            : (submission.pageArtifacts ?? []).find((page) => page.pageNumber === answerPageNumber) ??
              null;
        const current = grouped.get(answer.questionId) ?? [];
        current.push({
          studentRef: submission.studentRef,
          answer,
          originalTotalPoints: answer.totalPointsAwarded ?? 0,
          effectiveTotalPoints: score.effectiveTotalPoints,
          effectiveFeedbackText: feedback.effectiveFeedbackText,
          hasScoreOverride: score.hasOverride,
          hasFeedbackOverride: feedback.hasOverride,
          pageImagePath: answerPage?.imagePath ?? null,
          pageNumber: answerPage?.pageNumber ?? answerPageNumber
        });
        grouped.set(answer.questionId, current);
      }
    }

    return [...grouped.entries()]
      .map(([questionId, cards]) => ({
        questionId,
        questionNumber: questionNumber(questionId),
        promptText: questionPromptText(questionId),
        maxPoints: questionMaxPoints(questionId, cards),
        reviewed: reviewLookup(questionId),
        cards: [...cards].sort((left, right) => left.studentRef.localeCompare(right.studentRef))
      }))
      .sort((left, right) => left.questionNumber - right.questionNumber);
  })();

  $: if (moderationQuestions.length === 0) {
    selectedQuestionId = null;
  } else if (
    !selectedQuestionId ||
    !moderationQuestions.some((item) => item.questionId === selectedQuestionId)
  ) {
    selectedQuestionId = moderationQuestions[0]?.questionId ?? null;
  }

  $: selectedQuestion =
    moderationQuestions.find((item) => item.questionId === selectedQuestionId) ?? null;

  $: {
    const nextVisibleLanes: Record<string, number[]> = {};
    for (const question of moderationQuestions) {
      const occupied = occupiedLanes(question);
      const existing = visibleLanesByQuestion[question.questionId] ?? occupied;
      nextVisibleLanes[question.questionId] = [...new Set([...existing, ...occupied])].sort(
        (left, right) => right - left
      );
    }
    visibleLanesByQuestion = nextVisibleLanes;
  }

  $: {
    const nextOptimisticScores = { ...optimisticScores };
    let changed = false;
    for (const submission of workspaceState.studentWorkflow?.submissions ?? []) {
      for (const answer of submission.answers ?? []) {
        const key = cardKey(submission.studentRef, answer.questionId);
        const optimisticScore = nextOptimisticScores[key];
        if (optimisticScore === undefined) {
          continue;
        }
        const persistedScore =
          workspaceState.moderationState?.scoreOverrides?.find(
            (item) =>
              item.studentRef === submission.studentRef && item.questionId === answer.questionId
          )?.moderatedTotalPoints ??
          answer.totalPointsAwarded ??
          0;
        if (persistedScore === optimisticScore) {
          delete nextOptimisticScores[key];
          changed = true;
        }
      }
    }
    if (changed) {
      optimisticScores = nextOptimisticScores;
    }
  }

  $: selectedQuestionCards = selectedQuestion
    ? selectedQuestion.cards.map((card) => {
        const optimisticScore =
          optimisticScores[cardKey(card.studentRef, card.answer.questionId)];
        const effectiveTotalPoints = optimisticScore ?? card.effectiveTotalPoints;
        return {
          ...card,
          effectiveTotalPoints,
          hasScoreOverride: effectiveTotalPoints !== card.originalTotalPoints
        };
      })
    : [];

  $: scoreLanes = selectedQuestion
    ? [...new Set([
        ...selectedQuestionCards.map((card) => card.effectiveTotalPoints),
        ...(visibleLanesByQuestion[selectedQuestion.questionId] ?? [])
      ])].sort(
        (left, right) => right - left
      )
    : [];

  function laneCards(
    score: number,
    compactState: Record<string, boolean> = compactCardsByKey
  ): ModerationCard[] {
    return selectedQuestionCards
      .filter((card) => card.effectiveTotalPoints === score)
      .sort((left, right) => {
        const leftCompact = isCompactCard(left, compactState);
        const rightCompact = isCompactCard(right, compactState);
        if (leftCompact !== rightCompact) {
          return leftCompact ? 1 : -1;
        }
        return left.studentRef.localeCompare(right.studentRef);
      });
  }

  function occupiedLanes(question: ModerationQuestionGroup): number[] {
    return [...new Set(question.cards.map((card) => card.effectiveTotalPoints))].sort(
      (left, right) => right - left
    );
  }

  function availableLaneChoices(
    question: ModerationQuestionGroup,
    visibleLanes: number[] = visibleLanesByQuestion[question.questionId] ?? []
  ): number[] {
    const visible = new Set([...occupiedLanes(question), ...visibleLanes]);
    return Array.from({ length: question.maxPoints + 1 }, (_, index) => question.maxPoints - index).filter(
      (score) => !visible.has(score)
    );
  }

  function laneChoiceOptions(question: ModerationQuestionGroup): Array<{ value: string; label: string }> {
    return availableLaneChoices(question).map((score) => ({
      value: String(score),
      label: `${score} pt`
    }));
  }

  $: cardColumnWidthStyle = `${12 + cardSize / 10}rem`;
  $: cardMinHeightStyle = `${9 + cardSize / 16}rem`;
  $: textBlockMaxHeightStyle = `${Math.min(7.5, 4 + cardSize / 18)}rem`;
  $: imageBlockMaxHeightStyle = `${Math.min(8, 4.5 + cardSize / 16)}rem`;
  $: feedbackBlockMaxHeightStyle = `${Math.min(4.75, 3 + cardSize / 18)}rem`;
  $: evidenceFontSizeStyle = `${0.65 + cardSize / 420}rem`;
  $: evidenceLineHeightStyle = `${0.95 + cardSize / 320}rem`;
  $: feedbackFontSizeStyle = `${0.65 + cardSize / 400}rem`;
  $: feedbackLineHeightStyle = `${0.95 + cardSize / 300}rem`;

  function evidenceText(answer: StudentWorkflowAnswer): string {
    return answer.verifiedText ?? answer.rawParsedText ?? '[blank]';
  }

  function feedbackValue(card: ModerationCard): string {
    return feedbackDrafts[cardKey(card.studentRef, card.answer.questionId)] ?? card.effectiveFeedbackText;
  }

  function studentLabel(
    studentRef: string,
    revealNames = showStudentNames,
    displayNamesByRef: Record<string, string> = studentDisplayNamesByRef
  ): string {
    return revealNames ? (displayNamesByRef[studentRef]?.trim() || studentRef) : studentRef;
  }

  function isCompactCard(card: ModerationCard, compactState = compactCardsByKey): boolean {
    return compactState[cardKey(card.studentRef, card.answer.questionId)] ?? false;
  }

  function toggleCompactCard(card: ModerationCard): void {
    const key = cardKey(card.studentRef, card.answer.questionId);
    compactCardsByKey = {
      ...compactCardsByKey,
      [key]: !isCompactCard(card)
    };
  }

  function cardFlags(card: ModerationCard): string[] {
    const flags: string[] = [];
    if (card.answer.piiPrescreen?.containsPii) {
      flags.push('PII');
    }
    if (card.answer.manualGradingRequired) {
      flags.push('Manual');
    }
    if (card.answer.reviewRequired) {
      flags.push('Review');
    }
    return flags;
  }

  function cardFlagTone(flag: string): FeedbackTone {
    if (flag === 'PII' || flag === 'Review') {
      return 'warning';
    }
    if (flag === 'Manual') {
      return 'info';
    }
    return 'muted';
  }

  function editedBadgeText(card: ModerationCard): string | null {
    return card.hasFeedbackOverride ? 'Edited' : null;
  }

  function scoreEditTitle(card: ModerationCard): string | undefined {
    return card.hasScoreOverride ? `Edited * was ${card.originalTotalPoints}` : undefined;
  }

  function previewTitle(preview: CardPreview): string {
    const label = studentLabel(
      preview.card.studentRef,
      showStudentNames,
      studentDisplayNamesByRef
    );
    if (preview.kind === 'page') {
      return `${label} page ${preview.card.pageNumber ?? ''}`.trim();
    }
    return `${label} graded rubric`;
  }

  function previewAnchorStyle(kind: CardPreview['kind'], event: MouseEvent): string {
    const target = event.currentTarget;
    const article =
      target instanceof HTMLElement ? target.closest('article') ?? target : null;
    const rect = article?.getBoundingClientRect() ?? new DOMRect(16, 16, 0, 0);
    const margin = 12;
    const gap = 10;
    const popoverWidth = kind === 'page' ? 560 : 430;
    const popoverHeight = kind === 'page' ? 720 : 460;
    const viewportWidth = window.innerWidth;
    const viewportHeight = window.innerHeight;
    const width = Math.min(popoverWidth, Math.max(280, viewportWidth - margin * 2));
    const left =
      rect.right + gap + width <= viewportWidth - margin
        ? rect.right + gap
        : rect.left - gap - width >= margin
          ? rect.left - gap - width
          : Math.max(margin, Math.min(rect.right + gap, viewportWidth - margin - width));
    const availableBelow = viewportHeight - margin;
    const top = Math.max(
      margin,
      Math.min(rect.top, availableBelow - Math.min(popoverHeight, viewportHeight - margin * 2))
    );
    const maxHeight = Math.max(240, viewportHeight - top - margin);
    return `left:${left}px;top:${top}px;width:${width}px;max-height:${maxHeight}px;`;
  }

  function clearPreviewOpenSuppression(): void {
    if (previewOpenSuppressionTimer !== null) {
      window.clearTimeout(previewOpenSuppressionTimer);
    }
    previewOpenSuppressionTimer = null;
    previewOpenSuppressedTarget = null;
  }

  function suppressNextPreviewOpenFrom(target: Node): void {
    clearPreviewOpenSuppression();
    previewOpenSuppressedTarget = target;
    previewOpenSuppressionTimer = window.setTimeout(clearPreviewOpenSuppression, 500);
  }

  function openPreview(kind: CardPreview['kind'], card: ModerationCard, event: MouseEvent): void {
    const trigger = event.currentTarget instanceof HTMLElement ? event.currentTarget : null;
    if (
      trigger &&
      previewOpenSuppressedTarget &&
      trigger.contains(previewOpenSuppressedTarget)
    ) {
      clearPreviewOpenSuppression();
      return;
    }
    previewTriggerElement = trigger;
    activePreviewStyle = previewAnchorStyle(kind, event);
    activePreview = {
      kind,
      cardKey: cardKey(card.studentRef, card.answer.questionId),
      card
    };
  }

  function closePreview(): void {
    const trigger = previewTriggerElement;
    activePreview = null;
    activePreviewStyle = '';
    previewTriggerElement = null;
    if (trigger) {
      queueMicrotask(() => trigger.focus());
    }
  }

  function nextUnreviewedQuestionId(currentQuestionId: string): string | null {
    const current = moderationQuestions.find((question) => question.questionId === currentQuestionId);
    if (!current) {
      return null;
    }
    return (
      moderationQuestions.find(
        (question) => question.questionNumber > current.questionNumber && !question.reviewed
      )?.questionId ?? null
    );
  }

  function applyLocalCardScore(
    studentRef: string,
    questionId: string,
    targetScore: number
  ): void {
    moderationQuestions = moderationQuestions.map((question) => {
      if (question.questionId !== questionId) {
        return question;
      }
      return {
        ...question,
        cards: question.cards.map((card) =>
          card.studentRef === studentRef && card.answer.questionId === questionId
            ? {
                ...card,
                effectiveTotalPoints: targetScore,
                hasScoreOverride: targetScore !== card.originalTotalPoints
              }
            : card
        )
      };
    });
    lanesRenderKey += 1;
  }

  async function handleSaveScore(card: ModerationCard, targetScore: number) {
    if (!selectedQuestion || busy || pendingCardKey === cardKey(card.studentRef, card.answer.questionId)) {
      return;
    }
    if (targetScore === card.effectiveTotalPoints) {
      return;
    }
    const key = cardKey(card.studentRef, card.answer.questionId);
    const previousDisplayedScore = card.effectiveTotalPoints;
    const previousScore = optimisticScores[key];
    applyLocalCardScore(card.studentRef, card.answer.questionId, targetScore);
    optimisticScores = {
      ...optimisticScores,
      [key]: targetScore
    };
    visibleLanesByQuestion = {
      ...visibleLanesByQuestion,
      [card.answer.questionId]: [
        ...new Set([...(visibleLanesByQuestion[card.answer.questionId] ?? []), targetScore])
      ].sort((left, right) => right - left)
    };
    pendingCardKey = key;
    try {
      await onSaveModeratedScore?.(card.studentRef, card.answer.questionId, targetScore);
    } catch (error) {
      applyLocalCardScore(card.studentRef, card.answer.questionId, previousDisplayedScore);
      const nextScores = { ...optimisticScores };
      if (previousScore === undefined) {
        delete nextScores[key];
      } else {
        nextScores[key] = previousScore;
      }
      optimisticScores = nextScores;
      throw error;
    } finally {
      pendingCardKey = null;
    }
  }

  async function handleSaveFeedback(card: ModerationCard) {
    const key = cardKey(card.studentRef, card.answer.questionId);
    const hasDraft = Object.hasOwn(feedbackDrafts, key);
    const draft = feedbackDrafts[key] ?? card.effectiveFeedbackText;
    if (!hasDraft || busy || pendingFeedbackKey === key || !onSaveModeratedFeedback) {
      return;
    }
    if (draft === card.effectiveFeedbackText) {
      const nextDrafts = { ...feedbackDrafts };
      delete nextDrafts[key];
      feedbackDrafts = nextDrafts;
      return;
    }

    pendingFeedbackKey = key;
    let saved = false;
    try {
      await onSaveModeratedFeedback(card.studentRef, card.answer.questionId, draft);
      saved = true;
    } finally {
      pendingFeedbackKey = null;
      if (saved) {
        const nextDrafts = { ...feedbackDrafts };
        delete nextDrafts[key];
        feedbackDrafts = nextDrafts;
      }
    }
  }

  async function handleReviewToggle(reviewed: boolean) {
    if (!selectedQuestion || busy || pendingReviewQuestionId === selectedQuestion.questionId) {
      return;
    }
    const currentQuestionId = selectedQuestion.questionId;
    pendingReviewQuestionId = selectedQuestion.questionId;
    try {
      const persisted = await onSetQuestionReviewed?.(currentQuestionId, reviewed);
      if (reviewed && persisted === true) {
        selectedQuestionId = nextUnreviewedQuestionId(currentQuestionId) ?? selectedQuestionId;
      }
    } catch {
      // Keep the current question selected when review persistence does not complete.
    } finally {
      pendingReviewQuestionId = null;
    }
  }

  function isInteractiveCardTarget(target: EventTarget | null): boolean {
    return (
      target instanceof Element &&
      target.closest(
        'button, a, input, textarea, select, [role="button"], [contenteditable="true"]'
      ) !== null
    );
  }

  function scoreLaneAtPoint(clientX: number, clientY: number): { questionId: string; score: number } | null {
    const element = document.elementFromPoint(clientX, clientY);
    const laneElement = element?.closest<HTMLElement>('[data-moderation-score-lane]');
    const value = laneElement?.dataset.moderationScoreLane ?? '';
    const separatorIndex = value.lastIndexOf(':');
    if (separatorIndex < 1) {
      return null;
    }
    const questionId = value.slice(0, separatorIndex);
    const score = Number(value.slice(separatorIndex + 1));
    if (!questionId || !Number.isFinite(score)) {
      return null;
    }
    return { questionId, score };
  }

  function updatePointerHoverLane(state: CardPointerDragState): void {
    const lane = scoreLaneAtPoint(state.latestX, state.latestY);
    hoverLaneKey = lane ? laneKey(lane.questionId, lane.score) : null;
  }

  function releaseCardPointerCapture(element: HTMLElement, pointerId: number): void {
    if (typeof element.releasePointerCapture !== 'function') {
      return;
    }
    if (typeof element.hasPointerCapture === 'function' && !element.hasPointerCapture(pointerId)) {
      return;
    }
    element.releasePointerCapture(pointerId);
  }

  function handleCardPointerDown(event: PointerEvent, card: ModerationCard) {
    if (busy || event.button !== 0 || isInteractiveCardTarget(event.target)) {
      return;
    }
    event.preventDefault();
    pointerDragState = {
      cardKey: cardKey(card.studentRef, card.answer.questionId),
      pointerId: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
      latestX: event.clientX,
      latestY: event.clientY,
      dragging: false
    };
    const cardElement = event.currentTarget as HTMLElement;
    if (typeof cardElement.setPointerCapture === 'function') {
      cardElement.setPointerCapture(event.pointerId);
    }
  }

  function handleCardPointerMove(event: PointerEvent) {
    if (!pointerDragState || event.pointerId !== pointerDragState.pointerId) {
      return;
    }
    pointerDragState = {
      ...pointerDragState,
      latestX: event.clientX,
      latestY: event.clientY
    };
    const moved =
      Math.abs(pointerDragState.latestX - pointerDragState.startX) > 4 ||
      Math.abs(pointerDragState.latestY - pointerDragState.startY) > 4;
    if (!pointerDragState.dragging && moved) {
      pointerDragState = {
        ...pointerDragState,
        dragging: true
      };
      draggingCardKey = pointerDragState.cardKey;
    }
    if (pointerDragState.dragging) {
      event.preventDefault();
      updatePointerHoverLane(pointerDragState);
    }
  }

  async function handleCardPointerUp(event: PointerEvent) {
    if (!pointerDragState || event.pointerId !== pointerDragState.pointerId) {
      return;
    }
    const completedDrag = pointerDragState.dragging;
    const draggedKey = pointerDragState.cardKey;
    const lane = scoreLaneAtPoint(event.clientX, event.clientY);
    releaseCardPointerCapture(event.currentTarget as HTMLElement, event.pointerId);
    pointerDragState = null;
    hoverLaneKey = null;
    draggingCardKey = null;
    if (!completedDrag || !lane || !selectedQuestion || lane.questionId !== selectedQuestion.questionId) {
      return;
    }
    const card = selectedQuestion.cards.find(
      (item) => cardKey(item.studentRef, item.answer.questionId) === draggedKey
    );
    if (!card) {
      return;
    }
    await handleSaveScore(card, lane.score);
  }

  function handleCardPointerCancel(event: PointerEvent) {
    if (!pointerDragState || event.pointerId !== pointerDragState.pointerId) {
      return;
    }
    releaseCardPointerCapture(event.currentTarget as HTMLElement, event.pointerId);
    pointerDragState = null;
    hoverLaneKey = null;
    draggingCardKey = null;
  }

  function handleDragStart(event: DragEvent, card: ModerationCard) {
    draggingCardKey = cardKey(card.studentRef, card.answer.questionId);
    event.dataTransfer?.setData('text/plain', draggingCardKey);
    if (event.dataTransfer) {
      event.dataTransfer.effectAllowed = 'move';
    }
  }

  function handleDragEnd() {
    draggingCardKey = null;
    hoverLaneKey = null;
  }

  function handleLaneDragOver(event: DragEvent, questionId: string, score: number) {
    event.preventDefault();
    hoverLaneKey = laneKey(questionId, score);
    if (event.dataTransfer) {
      event.dataTransfer.dropEffect = 'move';
    }
  }

  function handleLaneDragLeave(questionId: string, score: number) {
    if (hoverLaneKey === laneKey(questionId, score)) {
      hoverLaneKey = null;
    }
  }

  function handleAddLaneSelection(question: ModerationQuestionGroup, selectedLane: number) {
    visibleLanesByQuestion = {
      ...visibleLanesByQuestion,
      [question.questionId]: [...new Set([...(visibleLanesByQuestion[question.questionId] ?? []), selectedLane])].sort(
        (left, right) => right - left
      )
    };
    lanesRenderKey += 1;
  }

  async function handleLaneDrop(event: DragEvent, score: number) {
    event.preventDefault();
    const draggedKey = event.dataTransfer?.getData('text/plain') || draggingCardKey;
    hoverLaneKey = null;
    draggingCardKey = null;
    if (!selectedQuestion || !draggedKey) {
      return;
    }
    const card = selectedQuestion.cards.find(
      (item) => cardKey(item.studentRef, item.answer.questionId) === draggedKey
    );
    if (!card) {
      return;
    }
    await handleSaveScore(card, score);
  }

  function handleWindowPointerDown(event: PointerEvent) {
    const target = event.target;
    if (
      activePreview &&
      previewPopoverRef &&
      !(target instanceof Node && previewPopoverRef.contains(target))
    ) {
      if (target instanceof Node) {
        suppressNextPreviewOpenFrom(target);
      }
      closePreview();
    }
  }

  function handleWindowKeyDown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      evidenceControlsOpen = false;
      closePreview();
    }
  }
</script>

<svelte:window on:pointerdown={handleWindowPointerDown} on:keydown={handleWindowKeyDown} />

<section class="flex h-full min-h-0 flex-col bg-surface-panel px-3 py-3">
  {#if moderationQuestions.length === 0}
    <div class="flex h-full items-center justify-center">
      <div class="w-full max-w-2xl rounded-xl border border-border-default bg-surface-card px-6 py-8">
        <div class="text-sm font-semibold text-text-primary">No moderation-ready answers yet</div>
        <p class="mt-2 text-sm text-text-secondary">
          Moderation opens once at least one answer has a successful crop artifact.
        </p>
      </div>
    </div>
  {:else if selectedQuestion}
    <div class="flex min-h-0 flex-1 flex-col gap-3">
      <div class="flex items-start justify-between gap-3">
        <div class="min-w-0 flex-1 pb-1">
          <div class="relative flex max-w-full items-center border-b border-border-default bg-surface-sidebar">
            <div class="min-w-0 flex-1 overflow-x-auto px-3">
              <div class="inline-flex min-w-max items-center">
                {#each moderationQuestions as question (question.questionId)}
                  {@const isActiveQuestion = selectedQuestionId === question.questionId}
                  <div
                    role="group"
                    aria-label={`Question ${question.questionNumber} moderation tab`}
                    class={`relative inline-flex min-w-12 shrink-0 items-center justify-center gap-1.5 px-4 pb-2.5 pt-2 text-sm transition-colors disabled:cursor-not-allowed disabled:opacity-50 ${
                      isActiveQuestion
                        ? 'text-text-primary'
                        : 'text-text-muted hover:text-text-secondary'
                    }`}
                  >
                    <span
                      class="absolute inset-x-0 bottom-0 h-[3px] rounded-t"
                      class:bg-primary={isActiveQuestion}
                      class:bg-transparent={!isActiveQuestion}
                    ></span>
                    <button
                      type="button"
                      class="relative z-10 flex items-center gap-1.5"
                      onclick={() => {
                        selectedQuestionId = question.questionId;
                      }}
                    >
                      <span class={isActiveQuestion ? 'font-semibold' : 'font-medium'}>Q{question.questionNumber}</span>
                    </button>
                    {#if isActiveQuestion}
                      <button
                        type="button"
                        class={tabActionButtonClass}
                        disabled={busy || pendingReviewQuestionId === question.questionId}
                        onclick={() => {
                          void handleReviewToggle(!question.reviewed);
                        }}
                      >
                        {question.reviewed ? 'Undo' : 'Accept'}
                      </button>
                    {:else if question.reviewed}
                      <span
                        class="text-[10px] font-semibold leading-none text-text-secondary"
                        aria-hidden="true"
                        title="Reviewed"
                        data-testid={`reviewed-tab-indicator-${question.questionId}`}
                      >
                        ✓
                      </span>
                    {/if}
                  </div>
                {/each}
              </div>
            </div>
            <div class="flex shrink-0 items-center">
              <DesktopPopover
                bind:open={moderationTipsOpen}
                rootClass="relative shrink-0"
                triggerClass="inline-flex h-10 items-center justify-center gap-1.5 px-2.5 text-xs font-medium text-text-muted transition-colors hover:text-text-secondary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus-ring"
                triggerLabel="Moderation tips"
                triggerAriaHaspopup="dialog"
                panelRole="dialog"
                panelAriaLabel="Moderation tips"
                panelClass="w-[min(42rem,calc(100vw-4rem))] max-w-[calc(100vw-4rem)] space-y-3 p-3"
                align="end"
                aria-label="Moderation tips"
              >
                <svelte:fragment slot="trigger">
                  <HugeiconsIcon icon={FileQuestionMarkIcon} size={16} strokeWidth={1.8} aria-hidden="true" />
                  <span>Moderation Tips</span>
                </svelte:fragment>
                <img
                  class="block h-auto w-full rounded-lg border border-border-subtle object-contain"
                  src="/moderation-question-selection-tips.png"
                  alt="Synthetic guide showing moderation question tabs, score lanes, outlier review, and accept action"
                />
                <p class="text-sm leading-5 text-workspace-text-secondary">
                  Pick one question, compare anonymous answers within the vertical score lanes, review outliers, then use Accept in the question tab once the lanes look consistent.
                </p>
              </DesktopPopover>
              <DesktopPopover
                bind:open={evidenceControlsOpen}
                rootClass="relative shrink-0"
                triggerClass="inline-flex h-10 w-10 items-center justify-center p-0 text-text-muted transition-colors hover:text-text-secondary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus-ring"
                triggerLabel="Moderation view settings"
                triggerAriaHaspopup="dialog"
                panelRole="dialog"
                panelAriaLabel="Moderation view settings"
                panelClass="w-64 p-3"
                align="end"
                aria-label="Moderation view settings"
              >
                <svelte:fragment slot="trigger">
                  <HugeiconsIcon icon={AccountSetting01Icon} size={16} strokeWidth={1.8} aria-hidden="true" />
                </svelte:fragment>

                <div class="text-[11px] font-semibold uppercase tracking-wide text-text-muted">
                  Evidence
                </div>
                <SegmentedControl
                  class="mt-2 w-full"
                  options={evidenceViewOptions}
                  value={evidenceView}
                  ariaLabel="Evidence view"
                  onChange={(value) => {
                    evidenceView = value as EvidenceViewMode;
                  }}
                />

                <div class="mt-3 border-t border-border-default pt-3">
                  <ToggleRow
                    checked={showStudentNames}
                    title="Actual student names"
                    description={showStudentNames
                      ? 'Show live LMS roster names on answer cards.'
                      : 'Keep moderation cards anonymous by default.'}
                    onToggle={(checked) => {
                      showStudentNames = checked;
                    }}
                  />
                </div>

                <label class="mt-3 block text-[11px] font-semibold uppercase tracking-wide text-text-muted">
                  Size
                  <input
                    class="mt-2 w-full accent-interaction-selected"
                    type="range"
                    min="0"
                    max="192"
                    step="1"
                    bind:value={cardSize}
                    aria-label="Answer card size"
                  />
                </label>
              </DesktopPopover>
            </div>
          </div>
        </div>

      </div>

      <div class="flex items-start justify-between gap-3">
        <div class="flex min-w-0 flex-1 items-center gap-3 text-sm text-text-secondary">
          <div class="shrink-0">
            {#if availableLaneChoices(selectedQuestion).length > 0}
              <IconSelectField
                ariaLabel="Add lane"
                dialogLabel="Add point lane"
                menuLabel="Add lane"
                title="Add point lane"
                value=""
                options={laneChoiceOptions(selectedQuestion)}
                icon={Add01Icon}
                iconSize={18}
                iconStrokeWidth={1.8}
                disabled={busy}
                menuClass="min-w-28"
                triggerClass="inline-flex h-9 w-10 items-center justify-center rounded-xl border border-border-default bg-surface-card-control text-text-primary transition-colors hover:bg-interaction-hover disabled:cursor-not-allowed disabled:opacity-50"
                onChange={(value) => handleAddLaneSelection(selectedQuestion, Number(value))}
              />
            {/if}
          </div>
          <div class="min-w-0 flex-1">
            <span class="font-semibold text-text-primary">
              Q{selectedQuestion.questionNumber}
            </span>
            <span> - {selectedQuestion.promptText}</span>
          </div>
        </div>
      </div>

      <div class="min-h-0 flex-1 overflow-y-auto">
        {#key `${selectedQuestion.questionId}:${lanesRenderKey}` }
          <div class="space-y-3 pb-4">
            {#each scoreLanes as score (score)}
              <div
                role="group"
                aria-label={`Score lane ${score}`}
                data-testid={`score-lane-${score}`}
                data-moderation-score-lane={scoreLaneDatasetValue(selectedQuestion.questionId, score)}
                class={`space-y-2 rounded-lg px-0 py-0 ${
                  hoverLaneKey === laneKey(selectedQuestion.questionId, score) ? 'bg-surface-card-subtle' : ''
                }`}
                ondragover={(event) => handleLaneDragOver(event, selectedQuestion.questionId, score)}
                ondragleave={() => handleLaneDragLeave(selectedQuestion.questionId, score)}
                ondrop={(event) => handleLaneDrop(event, score)}
              >
                <div class="border-t border-border-default"></div>
                <div class="pt-1 text-sm font-medium text-text-primary">
                  {score} {score === 1 ? 'point' : 'points'} ({laneCards(score).length} students)
                </div>

                <div
                  class="grid justify-start gap-2"
                  style:grid-template-columns={`repeat(auto-fill, minmax(min(100%, ${cardColumnWidthStyle}), ${cardColumnWidthStyle}))`}
                >
                  {#each laneCards(score, compactCardsByKey) as card (cardKey(card.studentRef, card.answer.questionId))}
                    {@const label = studentLabel(card.studentRef, showStudentNames, studentDisplayNamesByRef)}
                    {@const displayLabel = `${label}${card.hasScoreOverride ? '*' : ''}`}
                    {@const compactCard = isCompactCard(card, compactCardsByKey)}
                    <article
                      data-testid={`moderation-card-${card.studentRef}-${card.answer.questionId}`}
                      draggable={false}
                      class={`flex select-none flex-col rounded-lg border border-border-default bg-surface-card-subtle text-xs ${
                        compactCard ? 'gap-1 p-1.5' : 'gap-2 p-2'
                      } ${
                        compactCard ? 'w-fit max-w-full self-start justify-self-start' : 'w-full'
                      } ${
                        draggingCardKey === cardKey(card.studentRef, card.answer.questionId)
                          ? 'opacity-70'
                          : busy
                            ? ''
                            : 'cursor-grab active:cursor-grabbing'
                      }`}
                      style:min-height={compactCard ? '0' : cardMinHeightStyle}
                      onpointerdown={(event) => handleCardPointerDown(event, card)}
                      onpointermove={handleCardPointerMove}
                      onpointerup={handleCardPointerUp}
                      onpointercancel={handleCardPointerCancel}
                      ondragstart={(event) => handleDragStart(event, card)}
                      ondragend={handleDragEnd}
                    >
                      <div class="flex items-center justify-between gap-2">
                        <div class="flex min-h-6 min-w-0 flex-1 items-center">
                          <div
                            class="truncate font-semibold text-text-primary"
                            title={scoreEditTitle(card)}
                          >
                            {displayLabel}
                          </div>
                        </div>

                        <div class="flex flex-wrap items-center justify-end gap-1">
                          {#if editedBadgeText(card)}
                            <StatusBadge
                              tone="muted"
                              class="min-h-0 rounded-sm px-1.5 py-0 text-[10px] leading-5"
                            >
                              {editedBadgeText(card)}
                            </StatusBadge>
                          {/if}
                          {#each cardFlags(card) as flag (flag)}
                            <StatusBadge
                              tone={cardFlagTone(flag)}
                              class="min-h-0 rounded-sm px-1.5 py-0 text-[10px] leading-5"
                            >
                              {flag}
                            </StatusBadge>
                          {/each}
                          {#if card.pageImagePath}
                            <IconButton
                              size="compact"
                              variant="ghost"
                              class="h-6 w-6 rounded-full"
                              ariaLabel={`Preview full page for ${displayLabel}`}
                              title={`Preview full page for ${displayLabel}`}
                              onclick={(event: MouseEvent) => openPreview('page', card, event)}
                            >
                              <HugeiconsIcon icon={File02Icon} size={14} strokeWidth={1.8} aria-hidden="true" />
                            </IconButton>
                          {/if}
                          <IconButton
                            size="compact"
                            variant="ghost"
                            class="h-6 w-6 rounded-full"
                            ariaLabel={`Preview rubric for ${displayLabel}`}
                            title={`Preview rubric for ${displayLabel}`}
                            onclick={(event: MouseEvent) => openPreview('rubric', card, event)}
                          >
                            <HugeiconsIcon icon={CheckListIcon} size={14} strokeWidth={1.8} aria-hidden="true" />
                          </IconButton>
                          <IconButton
                            size="compact"
                            variant="ghost"
                            class="h-6 w-6 rounded-full"
                            ariaLabel={`${compactCard ? 'Use full card for' : 'Use mini card for'} ${displayLabel}`}
                            title={`${compactCard ? 'Use full card for' : 'Use mini card for'} ${displayLabel}`}
                            aria-pressed={compactCard}
                            onclick={() => toggleCompactCard(card)}
                          >
                            <HugeiconsIcon
                              icon={compactCard ? EyeIcon : SquareArrowShrink02Icon}
                              size={14}
                              strokeWidth={1.8}
                              aria-hidden="true"
                            />
                          </IconButton>
                        </div>
                      </div>

                      {#if !compactCard}
                        {#if evidenceView === 'text' || evidenceView === 'both'}
                          <div
                            class="select-none overflow-auto rounded-md bg-surface-canvas px-2 py-1.5 text-[11px] leading-4 text-text-primary"
                            style:max-height={textBlockMaxHeightStyle}
                            style:font-size={evidenceFontSizeStyle}
                            style:line-height={evidenceLineHeightStyle}
                          >
                            {#each markedTextSegments(evidenceText(card.answer), card.answer.highlights ?? []) as segment, index (`${card.studentRef}-${card.answer.questionId}-${index}`)}
                              <span
                                class={`${segment.kind ? highlightKindToClass(segment.kind) : ''} whitespace-pre-wrap break-words`}
                              >
                                {segment.text}
                              </span>
                            {/each}
                          </div>
                        {/if}

                        {#if (evidenceView === 'png' || evidenceView === 'both') && card.answer.cropImagePath}
                          <div class="overflow-hidden rounded-md bg-surface-canvas">
                            <img
                              src={toDesktopAssetUrl(card.answer.cropImagePath)}
                              alt={`${label} answer crop`}
                              class="w-full object-contain"
                              style:max-height={imageBlockMaxHeightStyle}
                              loading="lazy"
                              draggable="false"
                            />
                          </div>
                        {/if}

                        <textarea
                          class="min-h-14 w-full select-text resize-none overflow-auto rounded-md border border-border-default bg-surface-canvas px-2 py-1.5 text-[11px] leading-4 text-text-primary outline-none transition-colors focus:border-border-strong focus:ring-2 focus:ring-focus-ring"
                          placeholder="Feedback"
                          rows="3"
                          style:max-height={feedbackBlockMaxHeightStyle}
                          style:font-size={feedbackFontSizeStyle}
                          style:line-height={feedbackLineHeightStyle}
                          value={feedbackValue(card)}
                          disabled={busy || pendingFeedbackKey === cardKey(card.studentRef, card.answer.questionId)}
                          oninput={(event) => {
                            const target = event.currentTarget as HTMLTextAreaElement;
                            feedbackDrafts = {
                              ...feedbackDrafts,
                              [cardKey(card.studentRef, card.answer.questionId)]: target.value
                            };
                          }}
                          onblur={() => handleSaveFeedback(card)}
                        ></textarea>
                      {/if}
                    </article>
                  {/each}
                </div>
              </div>
            {/each}
          </div>
        {/key}
      </div>
    </div>
  {/if}

  {#if activePreview}
    <div class="pointer-events-none fixed inset-0 z-40">
      <div
        bind:this={previewPopoverRef}
        class="pointer-events-auto fixed flex flex-col rounded-lg border border-border-default bg-surface-overlay shadow-[var(--surface-shadow-strong)]"
        style={activePreviewStyle}
        role="dialog"
        aria-label={previewTitle(activePreview)}
      >
        <div class="flex items-center justify-between gap-3 border-b border-border-default px-4 py-3">
          <div class="min-w-0">
            <div class="truncate text-sm font-semibold text-text-primary">
              {previewTitle(activePreview)}
            </div>
            <div class="text-xs text-text-secondary">
              Q{activePreview.card.answer.questionNumber} - {activePreview.card.effectiveTotalPoints}/{activePreview.card.answer.questionMaxPoints ?? selectedQuestion?.maxPoints ?? 0} points
            </div>
          </div>
          <IconButton
            size="compact"
            variant="ghost"
            class="h-8 w-8 rounded-full"
            ariaLabel="Close preview"
            title="Close preview"
            onclick={closePreview}
          >
            <HugeiconsIcon icon={Cancel01Icon} size={16} strokeWidth={1.8} aria-hidden="true" />
          </IconButton>
        </div>

        <div class="min-h-0 flex-1 overflow-auto p-4">
          {#if activePreview.kind === 'page'}
            {#if activePreview.card.pageImagePath}
              <PagePreviewFrame
                src={toDesktopAssetUrl(activePreview.card.pageImagePath)}
                alt={`${studentLabel(activePreview.card.studentRef, showStudentNames, studentDisplayNamesByRef)} full page ${activePreview.card.pageNumber ?? ''}`.trim()}
                class="mx-auto w-fit max-h-[62vh] max-w-full rounded-md"
                imageClass="block max-h-[62vh] max-w-full object-contain"
              />
            {/if}
          {:else}
            <div class="mb-3 rounded-md border border-border-default bg-surface-card-subtle px-3 py-2 text-sm text-text-primary">
              Total: <span class="font-semibold">{activePreview.card.effectiveTotalPoints}</span> / {activePreview.card.answer.questionMaxPoints ?? selectedQuestion?.maxPoints ?? 0}
            </div>
            {#if activePreview.card.answer.criterionResults.length > 0}
              <div class="space-y-2">
                {#each activePreview.card.answer.criterionResults as criterion (`${criterion.criterionIndex}:${criterion.pointsAwarded}:${criterion.rationale}`)}
                  <div class="rounded-md border border-border-default bg-surface-card-subtle px-3 py-2">
                    <div class="flex items-start justify-between gap-3">
                      <div class="min-w-0 text-sm font-semibold text-text-primary">
                        {criterion.label?.trim() || `Criterion ${criterion.criterionIndex + 1}`}
                      </div>
                      <div class="shrink-0 text-xs font-semibold text-text-secondary">
                        {criterion.pointsAwarded}/{criterion.points}
                      </div>
                    </div>
                    <p class="mt-1 text-xs leading-5 text-text-secondary">
                      {criterion.rationale || 'No rationale provided.'}
                    </p>
                  </div>
                {/each}
              </div>
            {:else}
              <div class="rounded-md border border-border-default bg-surface-card-subtle px-3 py-4 text-sm text-text-secondary">
                Rubric details are unavailable for this answer.
              </div>
            {/if}
          {/if}
        </div>
      </div>
    </div>
  {/if}
</section>
