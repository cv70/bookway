import { StatusBar } from 'expo-status-bar';
import { useEffect, useRef, useState } from 'react';
import { StyleSheet, View } from 'react-native';
import { SafeAreaProvider, SafeAreaView } from 'react-native-safe-area-context';

import { eventReporter } from './src/analytics/eventReporter';
import {
  completeAction,
  createAction,
  createComment,
  deleteComment,
  createEntry,
  createJourney,
  createKnowledge,
  createPost,
  getCompanion,
  getComments,
  getFeed,
  getEntries,
  getJourney,
  getJourneys,
  getKnowledge,
  getNotifications,
  getPost,
  getRouteParticipations,
  getSocialContext,
  getToday,
  getWeeklyReview,
  joinRoute,
  markNotificationRead,
  publishPost,
  reportPost,
  setFollow,
  setPostReaction,
  updateAction,
  updateJourney,
  updateKnowledge,
  viewerUserId,
} from './src/api/client';
import { ActionDetailModal } from './src/components/ActionDetailModal';
import { CreateEntryModal } from './src/components/CreateEntryModal';
import { CreateJourneyModal } from './src/components/CreateJourneyModal';
import { CreateMenuModal } from './src/components/CreateMenuModal';
import { JourneyDetailModal } from './src/components/JourneyDetailModal';
import { NotificationsModal } from './src/components/NotificationsModal';
import { PostDetailModal } from './src/components/PostDetailModal';
import { ProfileSectionModal } from './src/components/ProfileSectionModal';
import { ReaderModal } from './src/components/ReaderModal';
import { ReadingLibraryModal } from './src/components/ReadingLibraryModal';
import { TabBar } from './src/components/TabBar';
import { fallbackFeed, fallbackJourneys, fallbackReadingBooks, fallbackToday } from './src/data/fallback';
import { DiscoverScreen } from './src/screens/DiscoverScreen';
import { JourneysScreen } from './src/screens/JourneysScreen';
import { ProfileScreen, type ProfileSection } from './src/screens/ProfileScreen';
import { TodayScreen } from './src/screens/TodayScreen';
import { colors } from './src/theme';
import {
  Action,
  ActionUpdate,
  Comment,
  CompanionBrief,
  CommunityPost,
  CreateActionInput,
  CreateEntryInput,
  CreateJourneyInput,
  CreateReadingBookInput,
  Feed,
  FeedItem,
  GrowthEntry,
  Journey,
  JourneyUpdate,
  KnowledgeResource,
  NotificationPage,
  ReaderSettings,
  ReadingBook,
  ReadingBookmark,
  ReportReason,
  ReviewAdjustmentSuggestion,
  TabKey,
  Today,
  UserNotification,
  WeeklyReview,
} from './src/types';

type EntryContext = { actionId?: string; journeyId?: string; durationMinutes?: number };

export default function App() {
  const currentUserId = viewerUserId();
  const [activeTab, setActiveTab] = useState<TabKey>('today');
  const [today, setToday] = useState<Today>(fallbackToday);
  const [journeys, setJourneys] = useState<Journey[]>(fallbackJourneys);
  const [feed, setFeed] = useState<Feed>(fallbackFeed);
  const [followingFeed, setFollowingFeed] = useState<Feed>(() => ({
    request_id: 'local-following-preview',
    items: [],
    meta: { sourced: 0, filtered: 0, selected: 0 },
  }));
  const [feedLoadingMore, setFeedLoadingMore] = useState(false);
  const [followingFeedLoadingMore, setFollowingFeedLoadingMore] = useState(false);
  const [entries, setEntries] = useState<GrowthEntry[]>([]);
  const [weeklyReview, setWeeklyReview] = useState<WeeklyReview>();
  const [companion, setCompanion] = useState<CompanionBrief>();
  const [readingBooks, setReadingBooks] = useState<ReadingBook[]>(fallbackReadingBooks);
  const [readingBookmarks, setReadingBookmarks] = useState<ReadingBookmark[]>([]);
  const [readerSettings, setReaderSettings] = useState<ReaderSettings>({ font_size: 18, line_height: 1.8, theme: 'light' });
  const [journeyActionsById, setJourneyActionsById] = useState<Record<string, Action[]>>({});
  const [likedPostIds, setLikedPostIds] = useState<Set<string>>(() => new Set(['post-reading']));
  const [bookmarkedPostIds, setBookmarkedPostIds] = useState<Set<string>>(() => new Set());
  const [joinedRouteIds, setJoinedRouteIds] = useState<Set<string>>(() => new Set());
  const [joiningRouteIds, setJoiningRouteIds] = useState<Set<string>>(() => new Set());
  const [routeParticipantCounts, setRouteParticipantCounts] = useState<Record<string, number>>({});
  const [notificationPage, setNotificationPage] = useState<NotificationPage>({ items: [], unread_count: 0 });
  const [notificationsLoading, setNotificationsLoading] = useState(false);
  const [notificationsLoadingMore, setNotificationsLoadingMore] = useState(false);
  const joiningRouteIdsRef = useRef(new Set<string>());
  const completingActionIdsRef = useRef(new Set<string>());
  const homeFeedLoadingMoreRef = useRef(false);
  const followingFeedLoadingMoreRef = useRef(false);
  const notificationOpenRequestRef = useRef(0);
  const notificationsLoadingMoreRef = useRef(false);
  const [followingAuthorIds, setFollowingAuthorIds] = useState<Set<string>>(() => new Set());
  const [commentsByPost, setCommentsByPost] = useState<Record<string, Comment[]>>({});
  const [commentNextCursorByPost, setCommentNextCursorByPost] = useState<Record<string, string | undefined>>({});
  const [loadingCommentPostIds, setLoadingCommentPostIds] = useState<Set<string>>(() => new Set());
  const [offline, setOffline] = useState(false);
  const [createMenuVisible, setCreateMenuVisible] = useState(false);
  const [creatingJourney, setCreatingJourney] = useState(false);
  const [entryContext, setEntryContext] = useState<EntryContext | null>(null);
  const [selectedActionId, setSelectedActionId] = useState<string>();
  const [selectedJourneyId, setSelectedJourneyId] = useState<string>();
  const [selectedPost, setSelectedPost] = useState<CommunityPost>();
  const [selectedPostAuthorId, setSelectedPostAuthorId] = useState<string>();
  const [profileSection, setProfileSection] = useState<ProfileSection>();
  const [notificationsVisible, setNotificationsVisible] = useState(false);
  const [openingNotificationId, setOpeningNotificationId] = useState<string>();
  const [failedNotificationId, setFailedNotificationId] = useState<string>();
  const [readingLibraryVisible, setReadingLibraryVisible] = useState(false);
  const [readerBookId, setReaderBookId] = useState<string>();
  const [readerActionId, setReaderActionId] = useState<string>();

  useEffect(() => {
    eventReporter.start();
    let mounted = true;
    Promise.allSettled([
      getToday(),
      getJourneys(),
      getFeed(),
      getFeed(undefined, undefined, 'following'),
      getEntries(),
      getWeeklyReview(),
      getCompanion(),
      getKnowledge({ kind: 'book' }),
      getNotifications(),
      getSocialContext(),
      getRouteParticipations(),
    ])
      .then(([todayResult, journeysResult, feedResult, followingResult, entriesResult, reviewResult, companionResult, knowledgeResult, notificationResult, socialResult, participationResult]) => {
        if (!mounted) return;
        if (todayResult.status === 'fulfilled') setToday(todayResult.value);
        if (journeysResult.status === 'fulfilled') setJourneys(journeysResult.value);
        if (feedResult.status === 'fulfilled') setFeed(feedResult.value);
        if (followingResult.status === 'fulfilled') {
          setFollowingFeed(followingResult.value);
        }
        if (entriesResult.status === 'fulfilled') setEntries(entriesResult.value);
        if (reviewResult.status === 'fulfilled') setWeeklyReview(reviewResult.value);
        if (companionResult.status === 'fulfilled') setCompanion(companionResult.value);
        if (knowledgeResult.status === 'fulfilled') {
          setReadingBooks(knowledgeResult.value.map(knowledgeResourceToReadingBook));
          setReadingBookmarks(knowledgeResult.value.flatMap((resource) => resource.bookmarks.map((chapterId) => ({
            book_id: resource.id,
            chapter_id: chapterId,
            created_at: resource.updated_at,
          }))));
        }
        if (notificationResult.status === 'fulfilled') setNotificationPage(notificationResult.value);
        if (socialResult.status === 'fulfilled') {
          setFollowingAuthorIds(new Set(socialResult.value.followed_author_ids));
        }
        if (participationResult.status === 'fulfilled') {
          setJoinedRouteIds(new Set(participationResult.value.map((item) => item.route_id)));
        }
        setOffline([
          todayResult,
          journeysResult,
          feedResult,
          followingResult,
          entriesResult,
          reviewResult,
          companionResult,
          knowledgeResult,
          notificationResult,
          socialResult,
          participationResult,
        ].some((result) => result.status === 'rejected'));
      });
    return () => {
      mounted = false;
      eventReporter.stop();
    };
  }, []);

  const selectedAction = selectedActionId ? findAction(today, journeyActionsById, selectedActionId) : undefined;
  const selectedJourney = journeys.find((journey) => journey.id === selectedJourneyId);
  const activeReadingBook = readingBooks.find((book) => book.id === readerBookId);
  const linkedReadingAction = readerActionId ? findAction(today, journeyActionsById, readerActionId) : undefined;
  const selectedJourneyActions = selectedJourney
    ? journeyActionsById[selectedJourney.id] ?? today.actions.filter((action) => action.journey_id === selectedJourney.id)
    : [];
  const savedPosts = feed.items
    .filter((item) => bookmarkedPostIds.has(item.post.id))
    .map((item) => item.post);

  const refreshCompanion = () => {
    getCompanion().then(setCompanion).catch(() => undefined);
  };

  const loadMoreFeed = async (surface: 'home' | 'following') => {
    const current = surface === 'home' ? feed : followingFeed;
    const cursor = current.meta.next_cursor;
    const loadingRef = surface === 'home' ? homeFeedLoadingMoreRef : followingFeedLoadingMoreRef;
    if (!cursor || loadingRef.current) return;

    loadingRef.current = true;
    (surface === 'home' ? setFeedLoadingMore : setFollowingFeedLoadingMore)(true);
    try {
      const next = await getFeed(undefined, cursor, surface);
      (surface === 'home' ? setFeed : setFollowingFeed)((existing) => mergeFeedPages(existing, next));
    } catch {
      // Keep the current page usable when a continuation request is interrupted.
      setOffline(true);
    } finally {
      loadingRef.current = false;
      (surface === 'home' ? setFeedLoadingMore : setFollowingFeedLoadingMore)(false);
    }
  };

  const refreshNotifications = async () => {
    setNotificationsLoading(true);
    try {
      setNotificationPage(await getNotifications());
    } catch {
      setOffline(true);
    } finally {
      setNotificationsLoading(false);
    }
  };

  const loadMoreNotifications = async () => {
    const cursor = notificationPage.next_cursor;
    if (!cursor || notificationsLoadingMoreRef.current) return;
    notificationsLoadingMoreRef.current = true;
    setNotificationsLoadingMore(true);
    try {
      const nextPage = await getNotifications(cursor);
      setNotificationPage((current) => current.next_cursor === cursor ? mergeNotificationPages(current, nextPage) : current);
    } catch {
      // Preserve the loaded inbox page: the cursor remains available for a retry.
      setOffline(true);
    } finally {
      notificationsLoadingMoreRef.current = false;
      setNotificationsLoadingMore(false);
    }
  };

  const openPostDetail = (post: CommunityPost, authorId?: string) => {
    eventReporter.track({ event_type: 'view', component_id: 'post-detail', content_id: post.id });
    setSelectedPost(post);
    setSelectedPostAuthorId(authorId);
    getComments(post.id)
      .then((page) => {
        setCommentsByPost((current) => ({ ...current, [post.id]: page.items }));
        setCommentNextCursorByPost((current) => ({ ...current, [post.id]: page.next_cursor }));
      })
      .catch(() => undefined);
  };

  const openNotifications = () => {
    notificationOpenRequestRef.current += 1;
    setOpeningNotificationId(undefined);
    setFailedNotificationId(undefined);
    setNotificationsVisible(true);
    void refreshNotifications();
  };

  const openNotification = (notification: UserNotification) => {
    const requestId = ++notificationOpenRequestRef.current;
    setFailedNotificationId(undefined);
    if (!notification.read_at) {
      const optimisticReadAt = new Date().toISOString();
      setNotificationPage((current) => ({
        ...current,
        unread_count: Math.max(0, current.unread_count - 1),
        items: current.items.map((item) => item.id === notification.id ? { ...item, read_at: optimisticReadAt } : item),
      }));
      markNotificationRead(notification.id)
        .then((updated) => setNotificationPage((current) => ({
          ...current,
          items: current.items.map((item) => item.id === notification.id ? updated : item),
        })))
        .catch(() => {
          setNotificationPage((current) => ({
            ...current,
            unread_count: current.unread_count + 1,
            items: current.items.map((item) => item.id === notification.id ? notification : item),
          }));
          setOffline(true);
        });
    }

    const actionId = typeof notification.data.action_id === 'string' ? notification.data.action_id : undefined;
    const journeyId = typeof notification.data.journey_id === 'string' ? notification.data.journey_id : undefined;
    const postId = typeof notification.data.post_id === 'string' ? notification.data.post_id : undefined;
    const appealId = typeof notification.data.appeal_id === 'string' ? notification.data.appeal_id : undefined;
    if (appealId) {
      setNotificationsVisible(false);
      setProfileSection('creation');
      setActiveTab('profile');
      return;
    }
    if (postId) {
      setOpeningNotificationId(notification.id);
      const existing = [...feed.items, ...followingFeed.items].find((item) => item.post.id === postId);
      if (existing) {
        if (requestId !== notificationOpenRequestRef.current) return;
        setOpeningNotificationId(undefined);
        setNotificationsVisible(false);
        openPostDetail(existing.post, existing.author_id || undefined);
        return;
      }
      getPost(postId)
        .then((content) => {
          if (requestId !== notificationOpenRequestRef.current) return;
          setOpeningNotificationId(undefined);
          setNotificationsVisible(false);
          openPostDetail(content.post, content.author_id);
        })
        .catch(() => {
          if (requestId !== notificationOpenRequestRef.current) return;
          setOpeningNotificationId(undefined);
          setFailedNotificationId(notification.id);
          setOffline(true);
        });
      return;
    }

    setNotificationsVisible(false);
    if (actionId) {
      getToday()
        .then((nextToday) => {
          setToday(nextToday);
          const action = nextToday.actions.find((item) => item.id === actionId);
          if (action) openAction(action);
          else setActiveTab('today');
        })
        .catch(() => setActiveTab('today'));
      return;
    }
    if (journeyId) {
      const journey = journeys.find((item) => item.id === journeyId);
      if (journey) openJourney(journey);
      else setActiveTab('journeys');
      return;
    }
    setActiveTab(notification.kind === 'community' ? 'discover' : 'today');
  };

  const replaceAction = (updated: Action) => {
    setToday((current) => summariseToday(current.actions.map((action) => action.id === updated.id ? updated : action)));
    setJourneyActionsById((current) => {
      const next = { ...current };
      if (next[updated.journey_id]) next[updated.journey_id] = next[updated.journey_id].map((action) => action.id === updated.id ? updated : action);
      return next;
    });
  };

  const handleComplete = async (actionId: string) => {
    const existing = findAction(today, journeyActionsById, actionId);
    if (!existing || completingActionIdsRef.current.has(actionId)) return false;
    if (existing.state === 'completed') return true;
    completingActionIdsRef.current.add(actionId);
    const completed = { ...existing, state: 'completed' as const };
    replaceAction(completed);
    eventReporter.track({ event_type: 'complete', component_id: 'today-action', content_id: actionId });
    try {
      const updated = await completeAction(actionId);
      replaceAction(updated);
      getToday().then(setToday).catch(() => undefined);
      getWeeklyReview().then(setWeeklyReview).catch(() => undefined);
      refreshCompanion();
      return true;
    } catch {
      replaceAction(existing);
      setOffline(true);
      return false;
    } finally {
      completingActionIdsRef.current.delete(actionId);
    }
  };

  const openReader = (book: ReadingBook, actionId?: string) => {
    setReadingLibraryVisible(false);
    setReaderBookId(book.id);
    setReaderActionId(actionId);
    eventReporter.track({ event_type: 'view', component_id: 'reader', content_id: book.id });
  };

  const openAction = (action: Action) => {
    const book = readingBooks.find((item) => item.journey_id === action.journey_id);
    if (book && isReadingAction(action)) {
      openReader(book, action.state === 'completed' ? undefined : action.id);
      return;
    }
    setSelectedActionId(action.id);
  };

  const handleCreateReadingBook = async (input: CreateReadingBookInput) => {
    const localBook = readingBookFromInput(input);
    try {
      const resource = await createKnowledge({
        title: input.title,
        creator: input.author,
        summary: localBook.summary,
        kind: 'book',
        status: 'active',
        body: input.content || localBook.chapters.flatMap((chapter) => chapter.body).join('\n\n'),
        tags: [],
      });
      const savedBook = knowledgeResourceToReadingBook(resource);
      setReadingBooks((current) => [savedBook, ...current]);
      openReader(savedBook);
    } catch (error) {
      setOffline(true);
      throw error;
    }
  };

  const handleSaveReadingProgress = (bookId: string, updates: Partial<Pick<ReadingBook, 'progress' | 'current_chapter' | 'last_opened_at' | 'reading_seconds'>>) => {
    setReadingBooks((current) => current.map((book) => book.id === bookId ? { ...book, ...updates } : book));
    updateKnowledge(bookId, {
      progress: updates.progress,
      current_position: updates.current_chapter,
      last_opened_at: updates.last_opened_at,
      reading_seconds: updates.reading_seconds,
      status: updates.progress === 100 ? 'completed' : 'active',
    }).catch(() => setOffline(true));
  };

  const handleToggleReadingBookmark = (bookId: string, chapterId: string) => {
    setReadingBookmarks((current) => {
      const existing = current.find((bookmark) => bookmark.book_id === bookId && bookmark.chapter_id === chapterId);
      const next = existing
        ? current.filter((bookmark) => bookmark !== existing)
        : [...current, { book_id: bookId, chapter_id: chapterId, created_at: new Date().toISOString() }];
      updateKnowledge(bookId, {
        bookmarks: next.filter((bookmark) => bookmark.book_id === bookId).map((bookmark) => bookmark.chapter_id),
      }).catch(() => setOffline(true));
      return next;
    });
    eventReporter.track({ event_type: 'bookmark', component_id: 'reader-bookmark', content_id: `${bookId}:${chapterId}` });
  };

  const closeReader = () => {
    setReaderBookId(undefined);
    setReaderActionId(undefined);
  };

  const handleUpdateAction = (actionId: string, updates: ActionUpdate) => {
    const existing = findAction(today, journeyActionsById, actionId);
    if (!existing) return;
    replaceAction({ ...existing, ...updates });
    updateAction(actionId, updates)
      .then((updated) => {
        replaceAction(updated);
        refreshCompanion();
      })
      .catch(() => undefined);
  };

  const handleCreateJourney = (input: CreateJourneyInput) => {
    setCreatingJourney(false);
    setCreateMenuVisible(false);
    const localJourney = journeyFromInput(input);
    const localAction = actionFromInput(localJourney.id, input, localJourney.stages);
    createJourney(input)
      .then(async (journey) => {
        setJourneys((current) => appendById(current, journey));
        try {
          setToday(await getToday());
          refreshCompanion();
        } catch {
          setToday((current) => summariseToday(appendById(current.actions, { ...localAction, journey_id: journey.id })));
        }
      })
      .catch(() => {
        setOffline(true);
        setJourneys((current) => appendById(current, localJourney));
        setToday((current) => summariseToday(appendById(current.actions, localAction)));
      });
    setActiveTab('journeys');
  };

  const handleJoinRoute = async (post: CommunityPost) => {
    if (joinedRouteIds.has(post.id) || joiningRouteIdsRef.current.has(post.id)) return;
    joiningRouteIdsRef.current.add(post.id);
    setJoiningRouteIds((current) => new Set(current).add(post.id));
    setJoinedRouteIds((current) => new Set(current).add(post.id));
    const input: CreateJourneyInput = {
      title: post.route_title || post.title,
      intent: post.summary,
      domain: post.domain,
      journey_type: 'project',
      completion_criteria: '完成路线中的必要阶段和行动',
      stages: [],
      duration_label: post.route_duration || '4 周',
      first_action_title: post.route_title || post.title,
      first_action_detail: post.summary,
      estimated_minutes: 20,
    };
    try {
      const result = await joinRoute(post.id);
      setJoinedRouteIds((current) => toggleId(current, post.id, result.participation.joined));
      setJourneys((current) => appendById(current, result.journey));
      try {
        setToday(await getToday());
        refreshCompanion();
      } catch {
        const localAction = actionFromInput(result.journey.id, input);
        setToday((current) => summariseToday(appendById(current.actions, localAction)));
      }
      setRouteParticipantCounts((current) => ({
        ...current,
        [post.id]: result.participation.participant_count,
      }));
      if (result.participation.joined) {
        eventReporter.track({ event_type: 'join_route', component_id: 'feed-route', content_id: post.id });
        setActiveTab('journeys');
      }
    } catch {
      setJoinedRouteIds((current) => toggleId(current, post.id, false));
      setOffline(true);
    } finally {
      joiningRouteIdsRef.current.delete(post.id);
      setJoiningRouteIds((current) => toggleId(current, post.id, false));
    }
  };

  const handleAddAction = (journeyId: string, input: CreateActionInput) => {
    const localAction: Action = {
      id: `local-action-${Date.now()}`,
      journey_id: journeyId,
      stage_id: input.stage_id,
      title: input.title,
      detail: input.detail,
      estimated_minutes: input.estimated_minutes,
      scheduled_label: input.scheduled_label,
      scheduled_for: input.scheduled_for,
      scheduled_timezone: input.scheduled_timezone,
      recurrence: input.recurrence,
      state: 'pending',
    };
    setToday((current) => summariseToday(appendById(current.actions, localAction)));
    setJourneyActionsById((current) => ({ ...current, [journeyId]: appendById(current[journeyId] ?? [], localAction) }));
    createAction(journeyId, input)
      .then((action) => {
        setToday((current) => summariseToday(current.actions.map((item) => item.id === localAction.id ? action : item)));
        setJourneyActionsById((current) => ({ ...current, [journeyId]: (current[journeyId] ?? []).map((item) => item.id === localAction.id ? action : item) }));
        refreshCompanion();
      })
      .catch(() => setOffline(true));
  };

  const handleUpdateJourney = (journeyId: string, updates: Partial<Journey>) => {
    const existing = journeys.find((journey) => journey.id === journeyId);
    if (!existing) return;
    const optimistic = { ...existing, ...updates };
    setJourneys((current) => current.map((journey) => journey.id === journeyId ? optimistic : journey));
    const request: JourneyUpdate = {};
    if (updates.title !== undefined) request.title = updates.title;
    if (updates.intent !== undefined) request.intent = updates.intent;
    if (updates.duration_label !== undefined) request.duration_label = updates.duration_label;
    if (updates.status !== undefined) request.status = updates.status;
    updateJourney(journeyId, request)
      .then((journey) => {
        setJourneys((current) => current.map((item) => item.id === journey.id ? journey : item));
        refreshCompanion();
      })
      .catch(() => setOffline(true));
  };

  const handleApplyReviewSuggestion = (suggestion: ReviewAdjustmentSuggestion) => {
    if (suggestion.action_patch) {
      const { action_id: actionId, estimated_minutes: estimatedMinutes, scheduled_label: scheduledLabel } = suggestion.action_patch;
      const updates: ActionUpdate = {};
      if (estimatedMinutes !== undefined) updates.estimated_minutes = estimatedMinutes;
      if (scheduledLabel !== undefined) updates.scheduled_label = scheduledLabel;
      updateAction(actionId, updates)
        .then(async (updated) => {
          replaceAction(updated);
          const [review, refreshedToday] = await Promise.allSettled([getWeeklyReview(), getToday()]);
          if (review.status === 'fulfilled') setWeeklyReview(review.value);
          if (refreshedToday.status === 'fulfilled') setToday(refreshedToday.value);
          refreshCompanion();
        })
        .catch(() => setOffline(true));
    }
    if (suggestion.journey_patch) {
      const { journey_id: journeyId, status } = suggestion.journey_patch;
      updateJourney(journeyId, { status })
        .then(async (journey) => {
          setJourneys((current) => current.map((item) => item.id === journey.id ? journey : item));
          const review = await getWeeklyReview();
          setWeeklyReview(review);
          refreshCompanion();
        })
        .catch(() => setOffline(true));
    }
  };

  const handleLike = (postId: string) => {
    const active = !likedPostIds.has(postId);
    setLikedPostIds((current) => toggleId(current, postId, active));
    setFeed((current) => ({
      ...current,
      items: current.items.map((item) => item.post.id === postId
        ? { ...item, post: { ...item.post, like_count: Math.max(0, item.post.like_count + (active ? 1 : -1)) } }
        : item),
    }));
    setFollowingFeed((current) => updateFeedLikeCount(current, postId, active));
    setPostReaction(postId, 'like', active).catch(() => setOffline(true));
    if (active) eventReporter.track({ event_type: 'like', component_id: 'feed-like', content_id: postId });
  };

  const handleBookmark = (postId: string) => {
    const active = !bookmarkedPostIds.has(postId);
    setBookmarkedPostIds((current) => toggleId(current, postId, active));
    setPostReaction(postId, 'bookmark', active).catch(() => setOffline(true));
    if (active) eventReporter.track({ event_type: 'bookmark', component_id: 'feed-bookmark', content_id: postId });
  };

  const handleHide = (postId: string) => {
    setFeed((current) => ({
      ...current,
      items: current.items.filter((item) => item.post.id !== postId),
    }));
    setFollowingFeed((current) => ({
      ...current,
      items: current.items.filter((item) => item.post.id !== postId),
    }));
    setSelectedPost((current) => current?.id === postId ? undefined : current);
    setPostReaction(postId, 'hide', true).catch(() => setOffline(true));
    eventReporter.track({ event_type: 'hide', component_id: 'feed-hide', content_id: postId });
  };

  const handleReport = async (postId: string, reason: ReportReason) => {
    try {
      await reportPost(postId, reason);
      eventReporter.track({ event_type: 'report', component_id: 'post-report', content_id: postId });
    } catch (error) {
      setOffline(true);
      throw error;
    }
  };

  const openPost = (item: FeedItem) => {
    openPostDetail(item.post, item.author_id || undefined);
  };

  const handleLoadMoreComments = async (postId: string) => {
    const cursor = commentNextCursorByPost[postId];
    if (!cursor || loadingCommentPostIds.has(postId)) return;
    setLoadingCommentPostIds((current) => new Set(current).add(postId));
    try {
      const page = await getComments(postId, cursor);
      setCommentsByPost((current) => ({
        ...current,
        [postId]: mergeComments(current[postId] ?? [], page.items),
      }));
      setCommentNextCursorByPost((current) => ({ ...current, [postId]: page.next_cursor }));
    } catch (error) {
      setOffline(true);
      throw error;
    } finally {
      setLoadingCommentPostIds((current) => toggleId(current, postId, false));
    }
  };

  const handleFollow = (post: CommunityPost) => {
    const authorId = selectedPost?.id === post.id ? selectedPostAuthorId : undefined;
    if (!authorId) return;
    const active = !followingAuthorIds.has(authorId);
    const previousFollowingFeed = followingFeed;
    setFollowingAuthorIds((current) => toggleId(current, authorId, active));
    if (!active) {
      setFollowingFeed((current) => ({
        ...current,
        items: current.items.filter((item) => item.author_id !== authorId),
      }));
    }
    setFollow(authorId, active)
      .then((context) => {
        setFollowingAuthorIds(new Set(context.followed_author_ids));
        getFeed(undefined, undefined, 'following')
          .then(setFollowingFeed)
          .catch(() => setOffline(true));
      })
      .catch(() => {
        setFollowingAuthorIds((current) => toggleId(current, authorId, !active));
        setFollowingFeed(previousFollowingFeed);
        setOffline(true);
      });
  };

  const handleComment = async (postId: string, body: string, parentId?: string) => {
    const localComment: Comment = {
      id: `local-comment-${Date.now()}`,
      post_id: postId,
      author_id: currentUserId ?? 'demo-user',
      author_name: '行路人',
      body,
      parent_id: parentId,
      like_count: 0,
      created_at: new Date().toISOString(),
      status: 'reviewing',
    };
    setCommentsByPost((current) => ({ ...current, [postId]: [...(current[postId] ?? []), localComment] }));
    try {
      const comment = await createComment(postId, body, parentId);
      setCommentsByPost((current) => ({
        ...current,
        [postId]: (current[postId] ?? []).map((item) => item.id === localComment.id ? comment : item),
      }));
      return comment;
    } catch (error) {
      setCommentsByPost((current) => ({
        ...current,
        [postId]: (current[postId] ?? []).filter((item) => item.id !== localComment.id),
      }));
      setOffline(true);
      throw error;
    }
  };

  const handleDeleteComment = async (postId: string, commentId: string) => {
    await deleteComment(postId, commentId);
    setCommentsByPost((current) => ({
      ...current,
      [postId]: (current[postId] ?? []).map((item) => item.id === commentId ? {
        ...item,
        author_id: '',
        author_name: '已删除用户',
        body: '该评论已删除',
        like_count: 0,
        status: 'deleted',
      } : item),
    }));
    try {
      const page = await getComments(postId);
      setCommentsByPost((current) => ({ ...current, [postId]: page.items }));
      setCommentNextCursorByPost((current) => ({ ...current, [postId]: page.next_cursor }));
    } catch {
      // The server-side delete succeeded; retain the local tombstone until refresh.
      setOffline(true);
    }
  };

  const handleSaveEntry = (input: CreateEntryInput) => {
    const entry: GrowthEntry = { ...input, id: `entry-${Date.now()}`, created_at: new Date().toISOString() };
    setEntries((current) => [entry, ...current]);
    setEntryContext(null);
    createEntry(input)
      .then((saved) => {
        setEntries((current) => current.map((item) => item.id === entry.id ? saved : item));
        getWeeklyReview().then(setWeeklyReview).catch(() => undefined);
        if (!saved.published) return;
        const journey = journeys.find((item) => item.id === saved.journey_id);
        const title = saved.body.slice(0, 24) || '一条新的行记';
        createPost({
          title,
          summary: saved.body.slice(0, 72),
          body: saved.body,
          domain: journey?.domain ?? 'leisure',
          content_type: 'note',
          cover_url: saved.photo_url,
          tags: [],
          topics: [],
          route_title: journey?.title,
          route_duration: journey?.duration_label,
        })
          .then((post) => publishPost(post.id))
          .catch(() => setOffline(true));
      })
      .catch(() => {
        setEntries((current) => current.filter((item) => item.id !== entry.id));
        setOffline(true);
      });
  };

  const handlePublishJourney = (journey: Journey) => {
    createPost({
      title: journey.title,
      summary: journey.intent,
      body: `${journey.intent}\n\n我正在走这条路线，欢迎根据自己的节奏调整。`,
      domain: journey.domain,
      content_type: 'route',
      tags: [],
      topics: [],
      route_title: journey.title,
      route_duration: journey.duration_label,
    })
      .then((post) => publishPost(post.id))
      .catch(() => setOffline(true));
  };

  const openJourney = (journey: Journey) => {
    setSelectedJourneyId(journey.id);
    getJourney(journey.id)
      .then((detail) => setJourneyActionsById((current) => ({ ...current, [journey.id]: detail.actions })))
      .catch(() => undefined);
  };

  const screen = {
    today: <TodayScreen companion={companion} journeys={journeys} notificationCount={notificationPage.unread_count} onComplete={handleComplete} onCreateJourney={() => setCreatingJourney(true)} onDiscover={() => setActiveTab('discover')} onNotifications={openNotifications} onOpenAction={openAction} today={today} />,
    discover: <DiscoverScreen bookmarkedPostIds={bookmarkedPostIds} feed={feed} feedLoadingMore={feedLoadingMore} followingFeed={followingFeed} followingFeedLoadingMore={followingFeedLoadingMore} joinedRouteIds={joinedRouteIds} joiningRouteIds={joiningRouteIds} likedPostIds={likedPostIds} offline={offline} onBookmark={handleBookmark} onHide={handleHide} onJoin={handleJoinRoute} onLike={handleLike} onLoadMoreFeed={loadMoreFeed} onOpen={openPost} routeParticipantCounts={routeParticipantCounts} />,
    journeys: <JourneysScreen journeys={journeys} onCreate={() => setCreatingJourney(true)} onOpen={openJourney} />,
    profile: <ProfileScreen entries={entries} journeys={journeys} onOpenLibrary={() => setReadingLibraryVisible(true)} onOpenSection={setProfileSection} today={today} />,
  }[activeTab];

  return (
    <SafeAreaProvider>
      <SafeAreaView edges={['top', 'left', 'right']} style={styles.safeArea}>
        <View style={styles.screen}>{screen}</View>
        <TabBar active={activeTab} onChange={setActiveTab} onCreate={() => setCreateMenuVisible(true)} />
      </SafeAreaView>
      <CreateMenuModal onClose={() => setCreateMenuVisible(false)} onCreateEntry={() => { setCreateMenuVisible(false); setEntryContext({}); }} onCreateJourney={() => { setCreateMenuVisible(false); setCreatingJourney(true); }} visible={createMenuVisible} />
      <CreateJourneyModal onClose={() => setCreatingJourney(false)} onSubmit={handleCreateJourney} visible={creatingJourney} />
      <CreateEntryModal actionId={entryContext?.actionId} initialDurationMinutes={entryContext?.durationMinutes} journeyId={entryContext?.journeyId} journeys={journeys} onClose={() => setEntryContext(null)} onSubmit={handleSaveEntry} visible={entryContext !== null} />
      <ActionDetailModal action={selectedAction} journeyTitle={journeys.find((journey) => journey.id === selectedAction?.journey_id)?.title} onClose={() => setSelectedActionId(undefined)} onComplete={handleComplete} onCreateEntry={(action, elapsedSeconds) => { setSelectedActionId(undefined); setEntryContext({ actionId: action.id, journeyId: action.journey_id, durationMinutes: elapsedSeconds > 0 ? Math.max(1, Math.round(elapsedSeconds / 60)) : undefined }); }} onUpdate={handleUpdateAction} visible={Boolean(selectedAction)} />
      <JourneyDetailModal actions={selectedJourneyActions} journey={selectedJourney} onAddAction={handleAddAction} onClose={() => setSelectedJourneyId(undefined)} onOpenAction={openAction} onPublish={handlePublishJourney} onUpdateJourney={handleUpdateJourney} visible={Boolean(selectedJourney)} />
      <PostDetailModal bookmarked={Boolean(selectedPost && bookmarkedPostIds.has(selectedPost.id))} comments={selectedPost ? commentsByPost[selectedPost.id] ?? [] : []} currentUserId={currentUserId} following={Boolean(selectedPostAuthorId && followingAuthorIds.has(selectedPostAuthorId))} hasMoreComments={Boolean(selectedPost && commentNextCursorByPost[selectedPost.id])} joinCount={selectedPost ? routeParticipantCounts[selectedPost.id] : undefined} joined={Boolean(selectedPost && joinedRouteIds.has(selectedPost.id))} joining={Boolean(selectedPost && joiningRouteIds.has(selectedPost.id))} liked={Boolean(selectedPost && likedPostIds.has(selectedPost.id))} loadingMoreComments={Boolean(selectedPost && loadingCommentPostIds.has(selectedPost.id))} onBookmark={handleBookmark} onClose={() => { setSelectedPost(undefined); setSelectedPostAuthorId(undefined); }} onComment={handleComment} onDeleteComment={handleDeleteComment} onFollow={handleFollow} onHide={handleHide} onJoin={handleJoinRoute} onLike={handleLike} onLoadMoreComments={handleLoadMoreComments} onReport={handleReport} post={selectedPost} visible={Boolean(selectedPost)} />
      <ProfileSectionModal entries={entries} journeys={journeys} onApplyReviewSuggestion={handleApplyReviewSuggestion} onClose={() => setProfileSection(undefined)} review={weeklyReview} savedPosts={savedPosts} section={profileSection} visible={Boolean(profileSection)} />
      <NotificationsModal failedNotificationId={failedNotificationId} loading={notificationsLoading} loadingMore={notificationsLoadingMore} nextCursor={notificationPage.next_cursor} notifications={notificationPage.items} onClose={() => { notificationOpenRequestRef.current += 1; setOpeningNotificationId(undefined); setNotificationsVisible(false); }} onLoadMore={() => void loadMoreNotifications()} onOpenNotification={openNotification} onRefresh={() => void refreshNotifications()} openingNotificationId={openingNotificationId} unreadCount={notificationPage.unread_count} visible={notificationsVisible} />
      <ReadingLibraryModal bookmarks={readingBookmarks} books={readingBooks} onClose={() => setReadingLibraryVisible(false)} onCreateBook={handleCreateReadingBook} onOpenBook={(book) => openReader(book)} visible={readingLibraryVisible} />
      <ReaderModal bookmarks={readingBookmarks} book={activeReadingBook} linkedAction={linkedReadingAction} onClose={closeReader} onCompleteAction={handleComplete} onSaveProgress={handleSaveReadingProgress} onToggleBookmark={handleToggleReadingBookmark} onUpdateSettings={(updates) => setReaderSettings((current) => ({ ...current, ...updates }))} settings={readerSettings} visible={Boolean(activeReadingBook)} />
      <StatusBar style={activeReadingBook && readerSettings.theme === 'night' ? 'light' : 'dark'} />
    </SafeAreaProvider>
  );
}

function summariseToday(actions: Action[]): Today {
  const completed = actions.filter((action) => action.state === 'completed');
  return {
    actions,
    completed: completed.length,
    total: actions.length,
    focus_minutes: completed.reduce((total, action) => total + action.estimated_minutes, 0),
  };
}

function findAction(today: Today, actionsByJourney: Record<string, Action[]>, actionId: string) {
  return today.actions.find((action) => action.id === actionId)
    ?? Object.values(actionsByJourney).flat().find((action) => action.id === actionId);
}

function appendById<T extends { id: string }>(items: T[], item: T) {
  return items.some((current) => current.id === item.id) ? items : [...items, item];
}

function mergeFeedPages(current: Feed, incoming: Feed): Feed {
  const knownIds = new Set(current.items.map((item) => item.post.id));
  const items = [...current.items, ...incoming.items.filter((item) => !knownIds.has(item.post.id))];
  return {
    ...incoming,
    items,
    meta: {
      ...incoming.meta,
      sourced: current.meta.sourced + incoming.meta.sourced,
      filtered: current.meta.filtered + incoming.meta.filtered,
      selected: items.length,
      degraded: Boolean(current.meta.degraded || incoming.meta.degraded),
    },
  };
}

function mergeNotificationPages(current: NotificationPage, incoming: NotificationPage): NotificationPage {
  const knownIds = new Set(current.items.map((item) => item.id));
  return {
    items: [...current.items, ...incoming.items.filter((item) => !knownIds.has(item.id))],
    next_cursor: incoming.next_cursor,
    unread_count: incoming.unread_count,
  };
}

function mergeComments(items: Comment[], incoming: Comment[]) {
  const known = new Set(items.map((item) => item.id));
  return [...items, ...incoming.filter((item) => !known.has(item.id))].sort((left, right) => {
    const timeDifference = commentTimestamp(left.created_at) - commentTimestamp(right.created_at);
    return timeDifference || left.id.localeCompare(right.id);
  });
}

function commentTimestamp(value: string) {
  const timestamp = /^\d+$/.test(value) ? Number(value) : new Date(value).getTime();
  return Number.isFinite(timestamp) ? timestamp : 0;
}

function toggleId(current: Set<string>, id: string, active: boolean) {
  const next = new Set(current);
  if (active) next.add(id);
  else next.delete(id);
  return next;
}

function journeyFromInput(input: CreateJourneyInput): Journey {
  return {
    id: `local-journey-${Date.now()}`,
    title: input.title,
    intent: input.intent,
    domain: input.domain,
    journey_type: input.journey_type,
    completion_criteria: input.completion_criteria,
    stages: input.stages.map((stage, position) => ({
      id: `local-stage-${Date.now()}-${position}`,
      ...stage,
      position,
    })),
    status: 'active',
    progress: 0,
    duration_label: input.duration_label,
    next_action: input.first_action_title,
    participant_count: 1,
  };
}

function actionFromInput(journeyId: string, input: CreateJourneyInput, stages: Journey['stages'] = []): Action {
  return {
    id: `local-action-${Date.now()}`,
    journey_id: journeyId,
    stage_id: input.first_action_stage_index === undefined ? undefined : stages[input.first_action_stage_index]?.id,
    title: input.first_action_title,
    detail: input.first_action_detail,
    estimated_minutes: input.estimated_minutes,
    scheduled_label: input.first_action_scheduled_label ?? '今天',
    scheduled_for: input.first_action_scheduled_for,
    scheduled_timezone: input.first_action_scheduled_timezone,
    recurrence: input.first_action_recurrence,
    state: 'pending',
  };
}

function isReadingAction(action: Action) {
  return /阅读|读书|章节/.test(action.title) || /阅读|书籍|章节/.test(action.detail);
}

function knowledgeResourceToReadingBook(resource: KnowledgeResource): ReadingBook {
  const paragraphs = (resource.body || resource.summary || '这项资源还没有正文，可以稍后补充内容。')
    .split(/\n\s*\n/)
    .map((paragraph) => paragraph.trim())
    .filter(Boolean);
  const chapters = chunk(paragraphs, 2).map((body, index) => ({
    id: `${resource.id}-chapter-${index}`,
    title: paragraphs.length > 2 ? `第 ${index + 1} 节` : '开始阅读',
    body,
  }));
  return {
    id: resource.id,
    title: resource.title,
    author: resource.creator,
    summary: resource.summary || paragraphs[0]?.slice(0, 54) || '',
    journey_id: resource.journey_id,
    progress: resource.progress,
    current_chapter: resource.current_position,
    reading_seconds: resource.reading_seconds,
    added_at: resource.created_at,
    last_opened_at: resource.last_opened_at,
    accent: knowledgeAccent(resource.id),
    chapters: chapters.length ? chapters : [{ id: `${resource.id}-chapter-0`, title: '开始阅读', body: ['这项资源还没有正文。'] }],
  };
}

function knowledgeAccent(id: string) {
  const palette = [colors.evergreen, colors.blue, colors.plum, colors.coral, colors.gold];
  const hash = Array.from(id).reduce((value, character) => value + character.charCodeAt(0), 0);
  return palette[hash % palette.length];
}

function updateFeedLikeCount(current: Feed, postId: string, active: boolean): Feed {
  return {
    ...current,
    items: current.items.map((item) => item.post.id === postId
      ? { ...item, post: { ...item.post, like_count: Math.max(0, item.post.like_count + (active ? 1 : -1)) } }
      : item),
  };
}

function readingBookFromInput(input: CreateReadingBookInput): ReadingBook {
  const paragraphs = input.content
    ? input.content.split(/\n\s*\n/).map((paragraph) => paragraph.trim()).filter(Boolean)
    : ['这是一本为自己建立的阅读文本。可以从一个问题出发，把之后想读到的片段和心得慢慢补进来。'];
  const chapters = chunk(paragraphs, 2).map((body, index) => ({
    id: `chapter-${Date.now()}-${index}`,
    title: paragraphs.length > 2 ? `第 ${index + 1} 节` : '开始阅读',
    body,
  }));
  const id = `book-${Date.now()}`;
  return {
    id,
    title: input.title,
    author: input.author || '行路人',
    summary: paragraphs[0].slice(0, 54),
    progress: 0,
    current_chapter: 0,
    reading_seconds: 0,
    added_at: new Date().toISOString(),
    accent: colors.plum,
    chapters,
  };
}

function chunk<T>(items: T[], size: number) {
  const groups: T[][] = [];
  for (let index = 0; index < items.length; index += size) groups.push(items.slice(index, index + size));
  return groups;
}

const styles = StyleSheet.create({
  safeArea: { flex: 1, backgroundColor: colors.background },
  screen: { flex: 1 },
});
