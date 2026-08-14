import { StatusBar } from 'expo-status-bar';
import { useEffect, useState } from 'react';
import { StyleSheet, View } from 'react-native';
import { SafeAreaProvider, SafeAreaView } from 'react-native-safe-area-context';

import { eventReporter } from './src/analytics/eventReporter';
import {
  completeAction,
  createAction,
  createComment,
  createJourney,
  createPost,
  getComments,
  getFeed,
  getJourney,
  getJourneys,
  getToday,
  publishPost,
  setFollow,
  setPostReaction,
  updateAction,
  updateJourney,
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
  CommunityPost,
  CreateActionInput,
  CreateEntryInput,
  CreateJourneyInput,
  CreateReadingBookInput,
  Feed,
  GrowthEntry,
  Journey,
  JourneyUpdate,
  ReaderSettings,
  ReadingBook,
  ReadingBookmark,
  TabKey,
  Today,
} from './src/types';

type EntryContext = { actionId?: string; journeyId?: string };

export default function App() {
  const [activeTab, setActiveTab] = useState<TabKey>('today');
  const [today, setToday] = useState<Today>(fallbackToday);
  const [journeys, setJourneys] = useState<Journey[]>(fallbackJourneys);
  const [feed, setFeed] = useState<Feed>(fallbackFeed);
  const [entries, setEntries] = useState<GrowthEntry[]>([]);
  const [readingBooks, setReadingBooks] = useState<ReadingBook[]>(fallbackReadingBooks);
  const [readingBookmarks, setReadingBookmarks] = useState<ReadingBookmark[]>([]);
  const [readerSettings, setReaderSettings] = useState<ReaderSettings>({ font_size: 18, line_height: 1.8, theme: 'light' });
  const [journeyActionsById, setJourneyActionsById] = useState<Record<string, Action[]>>({});
  const [likedPostIds, setLikedPostIds] = useState<Set<string>>(() => new Set(['post-reading']));
  const [bookmarkedPostIds, setBookmarkedPostIds] = useState<Set<string>>(() => new Set());
  const [joinedRouteIds, setJoinedRouteIds] = useState<Set<string>>(() => new Set());
  const [followingAuthorNames, setFollowingAuthorNames] = useState<Set<string>>(() => new Set());
  const [commentsByPost, setCommentsByPost] = useState<Record<string, Comment[]>>({});
  const [offline, setOffline] = useState(false);
  const [createMenuVisible, setCreateMenuVisible] = useState(false);
  const [creatingJourney, setCreatingJourney] = useState(false);
  const [entryContext, setEntryContext] = useState<EntryContext | null>(null);
  const [selectedActionId, setSelectedActionId] = useState<string>();
  const [selectedJourneyId, setSelectedJourneyId] = useState<string>();
  const [selectedPost, setSelectedPost] = useState<CommunityPost>();
  const [profileSection, setProfileSection] = useState<ProfileSection>();
  const [notificationsVisible, setNotificationsVisible] = useState(false);
  const [readingLibraryVisible, setReadingLibraryVisible] = useState(false);
  const [readerBookId, setReaderBookId] = useState<string>();
  const [readerActionId, setReaderActionId] = useState<string>();

  useEffect(() => {
    eventReporter.start();
    let mounted = true;
    Promise.all([getToday(), getJourneys(), getFeed()])
      .then(([nextToday, nextJourneys, nextFeed]) => {
        if (!mounted) return;
        setToday(nextToday);
        setJourneys(nextJourneys);
        setFeed(nextFeed);
        setOffline(false);
      })
      .catch(() => {
        if (mounted) setOffline(true);
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

  const replaceAction = (updated: Action) => {
    setToday((current) => summariseToday(current.actions.map((action) => action.id === updated.id ? updated : action)));
    setJourneyActionsById((current) => {
      const next = { ...current };
      if (next[updated.journey_id]) next[updated.journey_id] = next[updated.journey_id].map((action) => action.id === updated.id ? updated : action);
      return next;
    });
  };

  const handleComplete = (actionId: string) => {
    const existing = findAction(today, journeyActionsById, actionId);
    if (!existing || existing.state === 'completed') return;
    const completed = { ...existing, state: 'completed' as const };
    replaceAction(completed);
    eventReporter.track({ event_type: 'complete', component_id: 'today-action', content_id: actionId });
    completeAction(actionId).then(replaceAction).catch(() => undefined);
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

  const handleCreateReadingBook = (input: CreateReadingBookInput) => {
    const book = readingBookFromInput(input);
    setReadingBooks((current) => [book, ...current]);
    openReader(book);
  };

  const handleSaveReadingProgress = (bookId: string, updates: Partial<Pick<ReadingBook, 'progress' | 'current_chapter' | 'last_opened_at' | 'reading_seconds'>>) => {
    setReadingBooks((current) => current.map((book) => book.id === bookId ? { ...book, ...updates } : book));
  };

  const handleToggleReadingBookmark = (bookId: string, chapterId: string) => {
    setReadingBookmarks((current) => {
      const existing = current.find((bookmark) => bookmark.book_id === bookId && bookmark.chapter_id === chapterId);
      return existing
        ? current.filter((bookmark) => bookmark !== existing)
        : [...current, { book_id: bookId, chapter_id: chapterId, created_at: new Date().toISOString() }];
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
    updateAction(actionId, updates).then(replaceAction).catch(() => undefined);
  };

  const handleCreateJourney = (input: CreateJourneyInput) => {
    setCreatingJourney(false);
    setCreateMenuVisible(false);
    const localJourney = journeyFromInput(input);
    const localAction = actionFromInput(localJourney.id, input);
    createJourney(input)
      .then(async (journey) => {
        setJourneys((current) => appendById(current, journey));
        try {
          setToday(await getToday());
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

  const handleJoinRoute = (post: CommunityPost) => {
    if (joinedRouteIds.has(post.id)) return;
    setJoinedRouteIds((current) => new Set(current).add(post.id));
    handleCreateJourney({
      title: post.route_title || post.title,
      intent: post.summary,
      domain: post.domain,
      duration_label: post.route_duration || '4 周',
      first_action_title: post.route_title || post.title,
      first_action_detail: post.summary,
      estimated_minutes: 20,
    });
    eventReporter.track({ event_type: 'join_route', component_id: 'feed-route', content_id: post.id });
  };

  const handleAddAction = (journeyId: string, input: CreateActionInput) => {
    const localAction: Action = {
      id: `local-action-${Date.now()}`,
      journey_id: journeyId,
      title: input.title,
      detail: input.detail,
      estimated_minutes: input.estimated_minutes,
      scheduled_label: input.scheduled_label,
      state: 'pending',
    };
    setToday((current) => summariseToday(appendById(current.actions, localAction)));
    setJourneyActionsById((current) => ({ ...current, [journeyId]: appendById(current[journeyId] ?? [], localAction) }));
    createAction(journeyId, input)
      .then((action) => {
        setToday((current) => summariseToday(current.actions.map((item) => item.id === localAction.id ? action : item)));
        setJourneyActionsById((current) => ({ ...current, [journeyId]: (current[journeyId] ?? []).map((item) => item.id === localAction.id ? action : item) }));
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
      .then((journey) => setJourneys((current) => current.map((item) => item.id === journey.id ? journey : item)))
      .catch(() => setOffline(true));
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
    setPostReaction(postId, 'like', active).catch(() => setOffline(true));
    if (active) eventReporter.track({ event_type: 'like', component_id: 'feed-like', content_id: postId });
  };

  const handleBookmark = (postId: string) => {
    const active = !bookmarkedPostIds.has(postId);
    setBookmarkedPostIds((current) => toggleId(current, postId, active));
    setPostReaction(postId, 'bookmark', active).catch(() => setOffline(true));
    if (active) eventReporter.track({ event_type: 'bookmark', component_id: 'feed-bookmark', content_id: postId });
  };

  const openPost = (post: CommunityPost) => {
    setSelectedPost(post);
    getComments(post.id)
      .then((comments) => setCommentsByPost((current) => ({ ...current, [post.id]: comments })))
      .catch(() => undefined);
  };

  const handleFollow = (post: CommunityPost) => {
    const active = !followingAuthorNames.has(post.author_name);
    setFollowingAuthorNames((current) => toggleId(current, post.author_name, active));
    setFollow(`author-${post.id}`, active).catch(() => setOffline(true));
  };

  const handleComment = (postId: string, body: string) => {
    const localComment: Comment = {
      id: `local-comment-${Date.now()}`,
      post_id: postId,
      author_name: '行路人',
      body,
      created_at: new Date().toISOString(),
    };
    setCommentsByPost((current) => ({ ...current, [postId]: [...(current[postId] ?? []), localComment] }));
    createComment(postId, body)
      .then((comment) => setCommentsByPost((current) => ({
        ...current,
        [postId]: (current[postId] ?? []).map((item) => item.id === localComment.id ? comment : item),
      })))
      .catch(() => setOffline(true));
  };

  const handleSaveEntry = (input: CreateEntryInput) => {
    const entry: GrowthEntry = { ...input, id: `entry-${Date.now()}`, created_at: new Date().toISOString() };
    setEntries((current) => [entry, ...current]);
    setEntryContext(null);
    if (!entry.published) return;
    const journey = journeys.find((item) => item.id === entry.journey_id);
    const title = entry.body.slice(0, 24) || '一条新的行记';
    createPost({
      title,
      summary: entry.body.slice(0, 72),
      body: entry.body,
      domain: journey?.domain ?? 'leisure',
      content_type: 'note',
      cover_url: entry.photo_url,
      tags: [],
      topics: [],
      route_title: journey?.title,
      route_duration: journey?.duration_label,
    })
      .then((post) => publishPost(post.id))
      .catch(() => setOffline(true));
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
    today: <TodayScreen journeys={journeys} onComplete={handleComplete} onCreateJourney={() => setCreatingJourney(true)} onDiscover={() => setActiveTab('discover')} onNotifications={() => setNotificationsVisible(true)} onOpenAction={openAction} today={today} />,
    discover: <DiscoverScreen bookmarkedPostIds={bookmarkedPostIds} feed={feed} followingAuthorNames={followingAuthorNames} joinedRouteIds={joinedRouteIds} likedPostIds={likedPostIds} offline={offline} onBookmark={handleBookmark} onJoin={handleJoinRoute} onLike={handleLike} onOpen={openPost} />,
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
      <CreateEntryModal actionId={entryContext?.actionId} journeyId={entryContext?.journeyId} journeys={journeys} onClose={() => setEntryContext(null)} onSubmit={handleSaveEntry} visible={entryContext !== null} />
      <ActionDetailModal action={selectedAction} journeyTitle={journeys.find((journey) => journey.id === selectedAction?.journey_id)?.title} onClose={() => setSelectedActionId(undefined)} onComplete={handleComplete} onCreateEntry={(action) => { setSelectedActionId(undefined); setEntryContext({ actionId: action.id, journeyId: action.journey_id }); }} onUpdate={handleUpdateAction} visible={Boolean(selectedAction)} />
      <JourneyDetailModal actions={selectedJourneyActions} journey={selectedJourney} onAddAction={handleAddAction} onClose={() => setSelectedJourneyId(undefined)} onOpenAction={openAction} onPublish={handlePublishJourney} onUpdateJourney={handleUpdateJourney} visible={Boolean(selectedJourney)} />
      <PostDetailModal bookmarked={Boolean(selectedPost && bookmarkedPostIds.has(selectedPost.id))} comments={selectedPost ? commentsByPost[selectedPost.id] ?? [] : []} following={Boolean(selectedPost && followingAuthorNames.has(selectedPost.author_name))} joined={Boolean(selectedPost && joinedRouteIds.has(selectedPost.id))} liked={Boolean(selectedPost && likedPostIds.has(selectedPost.id))} onBookmark={handleBookmark} onClose={() => setSelectedPost(undefined)} onComment={handleComment} onFollow={handleFollow} onJoin={handleJoinRoute} onLike={handleLike} post={selectedPost} visible={Boolean(selectedPost)} />
      <ProfileSectionModal entries={entries} journeys={journeys} onClose={() => setProfileSection(undefined)} savedPosts={savedPosts} section={profileSection} visible={Boolean(profileSection)} />
      <NotificationsModal onClose={() => setNotificationsVisible(false)} visible={notificationsVisible} />
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
    status: 'active',
    progress: 0,
    duration_label: input.duration_label,
    next_action: input.first_action_title,
    participant_count: 1,
  };
}

function actionFromInput(journeyId: string, input: CreateJourneyInput): Action {
  return {
    id: `local-action-${Date.now()}`,
    journey_id: journeyId,
    title: input.first_action_title,
    detail: input.first_action_detail,
    estimated_minutes: input.estimated_minutes,
    scheduled_label: '今天',
    state: 'pending',
  };
}

function isReadingAction(action: Action) {
  return /阅读|读书|章节/.test(action.title) || /阅读|书籍|章节/.test(action.detail);
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
