import { StatusBar } from 'expo-status-bar';
import { useEffect, useRef, useState } from 'react';
import { StyleSheet, View } from 'react-native';
import { SafeAreaProvider, SafeAreaView } from 'react-native-safe-area-context';

import { eventReporter } from './src/analytics/eventReporter';
import {
  completeAction,
  capturePostAsKnowledge,
  createMallOrder,
  acceptQuestionAnswer,
  createAction,
  createComment,
  deleteComment,
  createEntry,
  createJourney,
  createKnowledge,
  getAccountProfile,
  getAuthorPosts,
getSocialStats,
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
  getRouteNodeOffers,
  payMallOrder,
  listMyOrders,
  getRouteNodeResources,
  getSocialContext,
  getToday,
  getWeeklyReview,
  forkRoute,
  joinRoute,
  markNotificationRead,
  retryEntryPublication,
  reportPost,
  setCreatorRelationship,
  setFollow,
  setPostReaction,
  startKnowledgeJourney,
  submitPostForReview,
  publishPost,
  updatePost,
  updateAction,
  updateJourney,
  updateKnowledge,
  viewerCanModerate,
  viewerUserId,
} from './src/api/client';
import { ActionDetailModal } from './src/components/ActionDetailModal';
import { AuthorProfileModal } from './src/components/AuthorProfileModal';
import { CreateEntryModal } from './src/components/CreateEntryModal';
import { CreateJourneyModal } from './src/components/CreateJourneyModal';
import { CreateMenuModal } from './src/components/CreateMenuModal';
import { CreatePostModal } from './src/components/CreatePostModal';
import { FeedbackModal } from './src/components/FeedbackModal';
import { ForkRouteModal } from './src/components/ForkRouteModal';
import { HideFeedbackModal } from './src/components/HideFeedbackModal';
import { JourneyDetailModal } from './src/components/JourneyDetailModal';
import { ModerationCommentsModal } from './src/components/ModerationCommentsModal';
import { MessagesModal } from './src/components/MessagesModal';
import { ResourceCatalogModal } from './src/components/ResourceCatalogModal';
import { NotificationsModal } from './src/components/NotificationsModal';
import { PostDetailModal } from './src/components/PostDetailModal';
import { ProfileSectionModal } from './src/components/ProfileSectionModal';
import { ReaderModal } from './src/components/ReaderModal';
import { RouteDraftModal } from './src/components/RouteDraftModal';
import { ReadingLibraryModal } from './src/components/ReadingLibraryModal';
import { TabBar } from './src/components/TabBar';
import { DiscoverScreen } from './src/screens/DiscoverScreen';
import { JourneysScreen } from './src/screens/JourneysScreen';
import { ProfileScreen, type ProfileSection } from './src/screens/ProfileScreen';
import { TodayScreen } from './src/screens/TodayScreen';
import { colors } from './src/theme';
import { attachFeedAttribution } from './src/utils/feedAttribution';
import {
  Action,
  AccountProfile,
  ActionUpdate,
  Comment,
  CompanionBrief,
  CommunityPost,
  ContentDetail,
  CreateActionInput,
  CreateEntryInput,
  CreateJourneyInput,
  CreateKnowledgeResourceInput,
  CreatePostInput,
  Feed,
  FeedActionContext,
  FeedItem,
  GrowthEntry,
  Journey,
  JourneyUpdate,
  KnowledgeResource,
  NotificationPage,
  NegativeFeedbackReason,
  MallOrder,
  NodeOffer,
  PublicAuthor,
  ReaderSettings,
  ReadingBook,
  ReadingBookmark,
  RecommendationEventContext,
  ReportReason,
  SocialStats,
  ReviewAdjustmentSuggestion,
  RouteTemplateAction,
  RouteNodeResourceAttachment,
  OwnedContent,
  TabKey,
  Today,
  UpdateKnowledgeResourceInput,
  UpdatePostInput,
  UserNotification,
  WeeklyReview,
} from './src/types';

type EntryContext = { actionId?: string; journeyId?: string; durationMinutes?: number };


export default function App() {
  const currentUserId = viewerUserId();
  const canModerate = viewerCanModerate();
  const [profile, setProfile] = useState<AccountProfile>(() => ({
    user_id: currentUserId ?? '',
    display_name: '行路人',
    avatar_url: '',
    bio: '在万卷与山河之间，成为自己',
    created_at: '',
    updated_at: '',
  }));
  const [activeTab, setActiveTab] = useState<TabKey>('today');
  const [today, setToday] = useState<Today>({ completed: 0, total: 0, focus_minutes: 0, actions: [] });
  const [journeys, setJourneys] = useState<Journey[]>([]);
  const [feed, setFeed] = useState<Feed>(() => ({ request_id: '', items: [], meta: { sourced: 0, filtered: 0, selected: 0 } }));
  const [contextualFeed, setContextualFeed] = useState<Feed>();
  const [contextualFeedContext, setContextualFeedContext] = useState<FeedActionContext>();
  const [contextualFeedLoading, setContextualFeedLoading] = useState(false);
  const [contextualOffers, setContextualOffers] = useState<NodeOffer[]>([]);
  const [myOrders, setMyOrders] = useState<MallOrder[]>([]);
  const [contextualOffersLoading, setContextualOffersLoading] = useState(false);
  const [contextualOffersError, setContextualOffersError] = useState(false);
  const [contextualAction, setContextualAction] = useState<RouteTemplateAction>();
  const [contextualResources, setContextualResources] = useState<RouteNodeResourceAttachment[]>([]);
  const [contextualResourcesLoading, setContextualResourcesLoading] = useState(false);
  const [contextualResourcesError, setContextualResourcesError] = useState(false);
  const [followingFeed, setFollowingFeed] = useState<Feed>(() => ({
    request_id: '',
    items: [],
    meta: { sourced: 0, filtered: 0, selected: 0 },
  }));
  const [feedLoadingMore, setFeedLoadingMore] = useState(false);
  const [followingFeedLoadingMore, setFollowingFeedLoadingMore] = useState(false);
  const [entries, setEntries] = useState<GrowthEntry[]>([]);
  const [retryingEntryIds, setRetryingEntryIds] = useState<Set<string>>(() => new Set());
  const [weeklyReview, setWeeklyReview] = useState<WeeklyReview>();
  const [companion, setCompanion] = useState<CompanionBrief>();
  const [readingBooks, setReadingBooks] = useState<ReadingBook[]>([]);
  const [knowledgeResources, setKnowledgeResources] = useState<KnowledgeResource[]>([]);
  const [readingBookmarks, setReadingBookmarks] = useState<ReadingBookmark[]>([]);
  const [readerSettings, setReaderSettings] = useState<ReaderSettings>({ font_size: 18, line_height: 1.8, theme: 'light' });
  const [journeyActionsById, setJourneyActionsById] = useState<Record<string, Action[]>>({});
  const [likedPostIds, setLikedPostIds] = useState<Set<string>>(() => new Set());
  const [bookmarkedPostIds, setBookmarkedPostIds] = useState<Set<string>>(() => new Set());
  const [joinedRouteIds, setJoinedRouteIds] = useState<Set<string>>(() => new Set());
  const [joiningRouteIds, setJoiningRouteIds] = useState<Set<string>>(() => new Set());
  const [routeParticipantCounts, setRouteParticipantCounts] = useState<Record<string, number>>({});
  const [notificationPage, setNotificationPage] = useState<NotificationPage>({ items: [], unread_count: 0 });
  const [notificationsLoading, setNotificationsLoading] = useState(false);
  const [notificationsLoadingMore, setNotificationsLoadingMore] = useState(false);
  const joiningRouteIdsRef = useRef(new Set<string>());
  const completingActionIdsRef = useRef(new Set<string>());
  const creatingJourneyFingerprintsRef = useRef(new Set<string>());
  const creatingActionFingerprintsRef = useRef(new Set<string>());
  const creatingEntryFingerprintsRef = useRef(new Set<string>());
  const journeyFallbackIdsByFingerprintRef = useRef(new Map<string, string>());
  const homeFeedLoadingMoreRef = useRef(false);
  const contextualFeedLoadingMoreRef = useRef(false);
  const followingFeedLoadingMoreRef = useRef(false);
  const contextualFeedRequestRef = useRef(0);
  const notificationOpenRequestRef = useRef(0);
  const postDetailRequestRef = useRef(0);
  const authorContentRequestRef = useRef(0);
  const authorContentLoadingMoreRef = useRef(false);
  const notificationsLoadingMoreRef = useRef(false);
  const [followingAuthorIds, setFollowingAuthorIds] = useState<Set<string>>(() => new Set());
  const [mutedAuthorIds, setMutedAuthorIds] = useState<Set<string>>(() => new Set());
  const [blockedAuthorIds, setBlockedAuthorIds] = useState<Set<string>>(() => new Set());
  const [commentsByPost, setCommentsByPost] = useState<Record<string, Comment[]>>({});
  const [commentNextCursorByPost, setCommentNextCursorByPost] = useState<Record<string, string | undefined>>({});
  const [loadingCommentPostIds, setLoadingCommentPostIds] = useState<Set<string>>(() => new Set());
  const [offline, setOffline] = useState(false);
  const [initialLoading, setInitialLoading] = useState(true);
  const [todayError, setTodayError] = useState(false);
  const [journeysError, setJourneysError] = useState(false);
  const [feedError, setFeedError] = useState(false);
  const [followingFeedError, setFollowingFeedError] = useState(false);
  const [entriesError, setEntriesError] = useState(false);
  const [ordersLoading, setOrdersLoading] = useState(true);
  const [ordersError, setOrdersError] = useState(false);
  const [createMenuVisible, setCreateMenuVisible] = useState(false);
  const [creatingJourney, setCreatingJourney] = useState(false);
  const [creatingPost, setCreatingPost] = useState(false);
  const [entryContext, setEntryContext] = useState<EntryContext | null>(null);
  const [selectedActionId, setSelectedActionId] = useState<string>();
  const [selectedJourneyId, setSelectedJourneyId] = useState<string>();
  const [selectedPost, setSelectedPost] = useState<CommunityPost>();
  const [selectedPostAuthorId, setSelectedPostAuthorId] = useState<string>();
  const [selectedContent, setSelectedContent] = useState<ContentDetail>();
  const [forkSourcePost, setForkSourcePost] = useState<CommunityPost>();
  const [selectedRouteDraft, setSelectedRouteDraft] = useState<ContentDetail | OwnedContent>();
  const [selectedPostRecommendationContext, setSelectedPostRecommendationContext] = useState<RecommendationEventContext>();
  const [hideFeedback, setHideFeedback] = useState<{ postId: string; context?: RecommendationEventContext }>();
  const [selectedAuthor, setSelectedAuthor] = useState<PublicAuthor>();
const [authorStats, setAuthorStats] = useState<SocialStats>();
  const [authorContents, setAuthorContents] = useState<ContentDetail[]>([]);
  const [authorContentsNextCursor, setAuthorContentsNextCursor] = useState<string>();
  const [authorContentsLoading, setAuthorContentsLoading] = useState(false);
  const [authorContentsLoadingMore, setAuthorContentsLoadingMore] = useState(false);
  const [authorContentsError, setAuthorContentsError] = useState<string>();
  const [profileSection, setProfileSection] = useState<ProfileSection>();
  const [feedbackVisible, setFeedbackVisible] = useState(false);
  const [moderationVisible, setModerationVisible] = useState(false);
  const [messagesVisible, setMessagesVisible] = useState(false);
  const [messageRecipientId, setMessageRecipientId] = useState<string>();
  const [resourceCatalogVisible, setResourceCatalogVisible] = useState(false);
  const [notificationsVisible, setNotificationsVisible] = useState(false);
  const [openingNotificationId, setOpeningNotificationId] = useState<string>();
  const [failedNotificationId, setFailedNotificationId] = useState<string>();
  const [readingLibraryVisible, setReadingLibraryVisible] = useState(false);
  const [readerBookId, setReaderBookId] = useState<string>();
  const [readerActionId, setReaderActionId] = useState<string>();

  const refreshMyOrders = () => {
    if (!currentUserId) {
      setOrdersLoading(false);
      return;
    }
    setOrdersLoading(true);
    listMyOrders()
      .then((response) => {
        setMyOrders(response.items);
        setOrdersError(false);
      })
      .catch(() => {
        setOrdersError(true);
        setOffline(true);
      })
      .finally(() => setOrdersLoading(false));
  };

  useEffect(() => {
    refreshMyOrders();
  }, []);

useEffect(() => {
    eventReporter.start();
    let mounted = true;
    Promise.allSettled([
      getAccountProfile(),
      getToday(),
      getJourneys(),
      getFeed(),
      getFeed(undefined, undefined, 'following'),
      getEntries(),
      getWeeklyReview(),
      getCompanion(),
      getKnowledge(),
      getNotifications(),
      getSocialContext(),
      getRouteParticipations(),
    ])
      .then(([profileResult, todayResult, journeysResult, feedResult, followingResult, entriesResult, reviewResult, companionResult, knowledgeResult, notificationResult, socialResult, participationResult]) => {
        if (!mounted) return;
        if (currentUserId) {
          if (profileResult.status === 'fulfilled') setProfile(profileResult.value);
          if (todayResult.status === 'fulfilled') setToday(todayResult.value);
          else setTodayError(true);
          if (journeysResult.status === 'fulfilled') setJourneys(journeysResult.value);
          else setJourneysError(true);
        }
        if (feedResult.status === 'fulfilled') setFeed(attachFeedAttribution(feedResult.value, 'home'));
        else setFeedError(true);
        if (followingResult.status === 'fulfilled') {
          setFollowingFeed(attachFeedAttribution(followingResult.value, 'following'));
        } else setFollowingFeedError(true);
        if (currentUserId) {
          if (entriesResult.status === 'fulfilled') setEntries(entriesResult.value);
          else setEntriesError(true);
          if (reviewResult.status === 'fulfilled') setWeeklyReview(reviewResult.value);
          if (companionResult.status === 'fulfilled') setCompanion(companionResult.value);
          if (knowledgeResult.status === 'fulfilled') {
            setKnowledgeResources(knowledgeResult.value);
            setReadingBooks(knowledgeResult.value
              .filter((resource) => resource.kind === 'book')
              .map(knowledgeResourceToReadingBook));
            setReadingBookmarks(knowledgeResult.value.flatMap((resource) => resource.bookmarks.map((chapterId) => ({
              book_id: resource.id,
              chapter_id: chapterId,
              created_at: resource.updated_at,
            }))));
          }
          if (notificationResult.status === 'fulfilled') setNotificationPage(notificationResult.value);
          if (socialResult.status === 'fulfilled') {
            setFollowingAuthorIds(new Set(socialResult.value.followed_author_ids));
            setMutedAuthorIds(new Set(socialResult.value.muted_author_ids));
            setBlockedAuthorIds(new Set(socialResult.value.blocked_author_ids));
          }
          if (participationResult.status === 'fulfilled') {
            setJoinedRouteIds(new Set(participationResult.value.map((item) => item.route_id)));
          }
        }
        setInitialLoading(false);
        const privateResults = [
          profileResult,
          todayResult,
          journeysResult,
          entriesResult,
          reviewResult,
          companionResult,
          knowledgeResult,
          notificationResult,
          socialResult,
          participationResult,
        ];
        setOffline(
          feedResult.status === 'rejected'
            || followingResult.status === 'rejected'
            || (Boolean(currentUserId) && privateResults.some((result) => result.status === 'rejected')),
        );
      });
    return () => {
      mounted = false;
      eventReporter.stop();
    };
  }, []);


  useEffect(() => {
    if (!entries.some((entry) => entryPublicationStatus(entry) === 'pending')) return undefined;
    const timer = setTimeout(() => {
      getEntries()
        .then((next) => {
          setEntries(next);
          setEntriesError(false);
        })
        .catch(() => setOffline(true));
    }, 4_000);
    return () => clearTimeout(timer);
  }, [entries]);

  const selectedAction = selectedActionId ? findAction(today, journeyActionsById, selectedActionId) : undefined;
  const selectedJourney = journeys.find((journey) => journey.id === selectedJourneyId);
  const activeReadingBook = readingBooks.find((book) => book.id === readerBookId);
  const linkedReadingAction = readerActionId ? findAction(today, journeyActionsById, readerActionId) : undefined;
  const selectedJourneyActions = selectedJourney
    ? journeyActionsById[selectedJourney.id] ?? today.actions.filter((action) => action.journey_id === selectedJourney.id)
    : [];
  const savedPosts = feed.items
    .filter((item): item is FeedItem & { post: CommunityPost } => {
      const post = item.post;
      return Boolean(post && bookmarkedPostIds.has(post.id));
    })
    .map((item) => item.post);

  const refreshCompanion = () => {
    getCompanion().then(setCompanion).catch(() => setOffline(true));
  };

  const loadMoreFeed = async (surface: 'home' | 'following') => {
    const contextual = surface === 'home' && contextualFeedContext ? contextualFeed : undefined;
    const current = contextual ?? (surface === 'home' ? feed : followingFeed);
    const cursor = current.meta.next_cursor;
    const loadingRef = contextual
      ? contextualFeedLoadingMoreRef
      : surface === 'home' ? homeFeedLoadingMoreRef : followingFeedLoadingMoreRef;
    if (!cursor || loadingRef.current) return;

    const requestId = contextual ? contextualFeedRequestRef.current : undefined;
    const actionContext = contextual ? contextualFeedContext : undefined;
    loadingRef.current = true;
    if (contextual) setContextualFeedLoading(true);
    else (surface === 'home' ? setFeedLoadingMore : setFollowingFeedLoadingMore)(true);
    try {
      const next = await getFeed(undefined, cursor, surface, actionContext);
      const attributed = attachFeedAttribution(next, surface);
      if (contextual) {
        if (requestId !== contextualFeedRequestRef.current) return;
        setContextualFeed((existing) => existing ? mergeFeedPages(existing, attributed) : attributed);
      } else {
        (surface === 'home' ? setFeed : setFollowingFeed)((existing) => mergeFeedPages(existing, attributed));
      }
    } catch {
      // Keep the current page usable when a continuation request is interrupted.
      if (!contextual || requestId === contextualFeedRequestRef.current) setOffline(true);
    } finally {
      loadingRef.current = false;
      if (contextual && requestId !== contextualFeedRequestRef.current) return;
      if (contextual) setContextualFeedLoading(false);
      else (surface === 'home' ? setFeedLoadingMore : setFollowingFeedLoadingMore)(false);
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

  const openPostDetail = (
    post: CommunityPost,
    authorId?: string,
    initialContent?: ContentDetail,
    recommendationContext?: RecommendationEventContext,
  ) => {
    const requestId = ++postDetailRequestRef.current;
    trackFeedEvent('view', post.id, recommendationContext);
    setSelectedPost(initialContent?.post ?? post);
    setSelectedPostAuthorId(initialContent?.author_id || authorId);
    setSelectedContent(initialContent);
    setSelectedPostRecommendationContext(recommendationContext);
    getComments(post.id)
      .then((page) => {
        setCommentsByPost((current) => ({ ...current, [post.id]: page.items }));
        setCommentNextCursorByPost((current) => ({ ...current, [post.id]: page.next_cursor }));
      })
      .catch(() => setOffline(true));
    if (initialContent) return;
    getPost(post.id)
      .then((content) => {
        if (requestId !== postDetailRequestRef.current) return;
        setSelectedContent(content);
        if (content.post) {
          setSelectedPost(content.post);
          setSelectedPostAuthorId(content.author_id || authorId);
        }
      })
      .catch(() => {
        if (requestId === postDetailRequestRef.current) setOffline(true);
      });
  };

  const openForkRoute = (post: CommunityPost) => {
    setForkSourcePost(post);
  };

  const handleForkRoute = async (title: string, summary: string) => {
    const source = forkSourcePost;
    if (!source) return;
    try {
      const draft = await forkRoute(source.id, title, summary);
      setForkSourcePost(undefined);
      closePostDetail();
      setSelectedRouteDraft(draft);
    } catch (error) {
      setOffline(true);
      throw error;
    }
  };

  const openRouteDraft = (content: OwnedContent) => {
    setProfileSection(undefined);
    setSelectedRouteDraft(content);
  };

  const saveRouteDraft = async (contentId: string, input: UpdatePostInput) => {
    try {
      const updated = await updatePost(contentId, input);
      setSelectedRouteDraft(updated);
      return updated;
    } catch (error) {
      setOffline(true);
      throw error;
    }
  };

  const publishRouteDraft = async (contentId: string, input: UpdatePostInput) => {
    try {
      const updated = await updatePost(contentId, input);
      setSelectedRouteDraft(updated);
      await publishPost(contentId, `route-publish-${contentId}`);
      setSelectedRouteDraft(undefined);
      setProfileSection('creation');
    } catch (error) {
      setOffline(true);
      throw error;
    }
  };

  const openAuthorProfile = (author: PublicAuthor) => {
    if (!author.id.trim()) return;
    const requestId = ++authorContentRequestRef.current;
    setSelectedAuthor(author);
    setAuthorStats(undefined);
    setAuthorContents([]);
    setAuthorContentsNextCursor(undefined);
    setAuthorContentsError(undefined);
    setAuthorContentsLoading(true);
    getSocialStats(author.id)
      .then((stats) => {
        if (requestId === authorContentRequestRef.current) setAuthorStats(stats);
      })
      .catch(() => {
        // Counts stay hidden rather than showing a stale guess.
      });
    getAuthorPosts(author.id)
      .then((page) => {
        if (requestId !== authorContentRequestRef.current) return;
        setAuthorContents(page.items);
        setAuthorContentsNextCursor(page.next_cursor ?? undefined);
      })
      .catch(() => {
        if (requestId === authorContentRequestRef.current) setAuthorContentsError('暂时无法读取这位创作者的公开内容。');
      })
      .finally(() => {
        if (requestId === authorContentRequestRef.current) setAuthorContentsLoading(false);
      });
  };

  const openPostAuthorProfile = (post: CommunityPost) => {
    const authorId = selectedPost?.id === post.id ? selectedPostAuthorId : undefined;
    if (!authorId) return;
    openAuthorProfile({ id: authorId, name: post.author_name, avatar_url: post.author_avatar_url });
  };

  const closeAuthorProfile = () => {
    authorContentRequestRef.current += 1;
    authorContentLoadingMoreRef.current = false;
    setSelectedAuthor(undefined);
    setAuthorStats(undefined);
    setAuthorContents([]);
    setAuthorContentsNextCursor(undefined);
    setAuthorContentsError(undefined);
    setAuthorContentsLoading(false);
    setAuthorContentsLoadingMore(false);
  };

  const loadMoreAuthorContents = () => {
    const author = selectedAuthor;
    const cursor = authorContentsNextCursor;
    if (!author || !cursor || authorContentLoadingMoreRef.current) return;
    const requestId = authorContentRequestRef.current;
    authorContentLoadingMoreRef.current = true;
    setAuthorContentsLoadingMore(true);
    getAuthorPosts(author.id, cursor)
      .then((page) => {
        if (requestId !== authorContentRequestRef.current) return;
        setAuthorContents((current) => page.items.reduce((items, item) => appendById(items, item), current));
        setAuthorContentsNextCursor(page.next_cursor ?? undefined);
      })
      .catch(() => {
        if (requestId === authorContentRequestRef.current) setAuthorContentsError('更多公开内容暂时无法读取。');
      })
      .finally(() => {
        if (requestId === authorContentRequestRef.current) {
          authorContentLoadingMoreRef.current = false;
          setAuthorContentsLoadingMore(false);
        }
      });
  };

  const openNotifications = () => {
    notificationOpenRequestRef.current += 1;
    setOpeningNotificationId(undefined);
    setFailedNotificationId(undefined);
    setNotificationsVisible(true);
    void refreshNotifications();
  };

  const openMessages = (recipientId?: string) => {
    setMessageRecipientId(recipientId);
    setMessagesVisible(true);
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
      const existing = [...feed.items, ...followingFeed.items].find((item) => item.post?.id === postId);
      if (existing?.post) {
        if (requestId !== notificationOpenRequestRef.current) return;
        setOpeningNotificationId(undefined);
        setNotificationsVisible(false);
        openPostDetail(existing.post, existing.author_id || undefined);
        return;
      }
      getPost(postId)
        .then((content) => {
          if (requestId !== notificationOpenRequestRef.current) return;
          if (!content.post) {
            setOpeningNotificationId(undefined);
            setFailedNotificationId(notification.id);
            return;
          }
          setOpeningNotificationId(undefined);
          setNotificationsVisible(false);
          openPostDetail(content.post, content.author_id, content);
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
          setTodayError(false);
          const action = nextToday.actions.find((item) => item.id === actionId);
          if (action) openAction(action);
          else setActiveTab('today');
        })
        .catch(() => {
          setTodayError(true);
          setOffline(true);
          setActiveTab('today');
        });
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
    try {
      const updated = await completeAction(actionId);
      replaceAction(updated);
      getToday()
        .then((next) => {
          setToday(next);
          setTodayError(false);
        })
        .catch(() => setOffline(true));
      getWeeklyReview().then(setWeeklyReview).catch(() => setOffline(true));
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

  const handleCreateKnowledgeResource = async (input: CreateKnowledgeResourceInput) => {
    try {
      const resource = await createKnowledge(input);
      setKnowledgeResources((current) => upsertById(current, resource));
      if (resource.kind === 'book' && resource.status === 'active') {
        const savedBook = knowledgeResourceToReadingBook(resource);
        setReadingBooks((current) => [savedBook, ...current.filter((book) => book.id !== savedBook.id)]);
        openReader(savedBook);
      }
    } catch (error) {
      setOffline(true);
      throw error;
    }
  };

  const handleUpdateKnowledgeResource = async (resourceId: string, input: UpdateKnowledgeResourceInput) => {
    try {
      const resource = await updateKnowledge(resourceId, input);
      setKnowledgeResources((current) => upsertById(current, resource));
    } catch (error) {
      setOffline(true);
      throw error;
    }
  };

  const handleSaveReadingProgress = (bookId: string, updates: Partial<Pick<ReadingBook, 'progress' | 'current_chapter' | 'last_opened_at' | 'reading_seconds'>>) => {
    setReadingBooks((current) => current.map((book) => book.id === bookId ? { ...book, ...updates } : book));
    setKnowledgeResources((current) => current.map((resource) => resource.id === bookId ? {
      ...resource,
      progress: updates.progress ?? resource.progress,
      current_position: updates.current_chapter ?? resource.current_position,
      last_opened_at: updates.last_opened_at ?? resource.last_opened_at,
      reading_seconds: updates.reading_seconds ?? resource.reading_seconds,
      status: updates.progress === 100 ? 'completed' : 'active',
    } : resource));
    updateKnowledge(bookId, {
      progress: updates.progress,
      current_position: updates.current_chapter,
      last_opened_at: updates.last_opened_at,
      reading_seconds: updates.reading_seconds,
      status: updates.progress === 100 ? 'completed' : 'active',
    }).then((resource) => setKnowledgeResources((current) => upsertById(current, resource)))
      .catch(() => setOffline(true));
  };

  const handleToggleReadingBookmark = (bookId: string, chapterId: string) => {
    setReadingBookmarks((current) => {
      const existing = current.find((bookmark) => bookmark.book_id === bookId && bookmark.chapter_id === chapterId);
      const next = existing
        ? current.filter((bookmark) => bookmark !== existing)
        : [...current, { book_id: bookId, chapter_id: chapterId, created_at: new Date().toISOString() }];
      const bookmarks = next.filter((bookmark) => bookmark.book_id === bookId).map((bookmark) => bookmark.chapter_id);
      setKnowledgeResources((current) => current.map((resource) => resource.id === bookId ? { ...resource, bookmarks } : resource));
      updateKnowledge(bookId, {
        bookmarks,
      }).then((resource) => setKnowledgeResources((current) => upsertById(current, resource)))
        .catch(() => setOffline(true));
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
      .catch(() => setOffline(true));
  };

  const handleCreateJourney = (input: CreateJourneyInput) => {
    const fingerprint = JSON.stringify(input);
    if (creatingJourneyFingerprintsRef.current.has(fingerprint)) return;
    creatingJourneyFingerprintsRef.current.add(fingerprint);
    setCreatingJourney(false);
    setCreateMenuVisible(false);
    const localJourney = journeyFromInput(input);
    const localAction = actionFromInput(localJourney.id, input, localJourney.stages);
    createJourney(input)
      .then(async (journey) => {
        const fallbackId = journeyFallbackIdsByFingerprintRef.current.get(fingerprint);
        setJourneys((current) => fallbackId
          ? current.map((item) => item.id === fallbackId ? journey : item)
          : appendById(current, journey));
        journeyFallbackIdsByFingerprintRef.current.delete(fingerprint);
        try {
          setToday(await getToday());
          refreshCompanion();
        } catch {
          setToday((current) => summariseToday(fallbackId
            ? current.actions.map((action) => action.journey_id === fallbackId
              ? { ...action, journey_id: journey.id }
              : action)
            : appendById(current.actions, { ...localAction, journey_id: journey.id })));
        }
      })
      .catch(() => {
        setOffline(true);
        if (!journeyFallbackIdsByFingerprintRef.current.has(fingerprint)) {
          journeyFallbackIdsByFingerprintRef.current.set(fingerprint, localJourney.id);
          setJourneys((current) => appendById(current, localJourney));
          setToday((current) => summariseToday(appendById(current.actions, localAction)));
        }
      })
      .finally(() => creatingJourneyFingerprintsRef.current.delete(fingerprint));
    setActiveTab('journeys');
  };

  const handleStartKnowledgeJourney = async (resource: KnowledgeResource) => {
    try {
      const result = await startKnowledgeJourney(resource.id, journeyFromKnowledgeResource(resource));
      setKnowledgeResources((current) => upsertById(current, result.resource));
      setJourneys((current) => appendById(current, result.journey.journey));
      setJourneyActionsById((current) => ({ ...current, [result.journey.journey.id]: result.journey.actions }));
      const firstAction = result.journey.actions[0];
      if (firstAction) setToday((current) => summariseToday(appendById(current.actions, firstAction)));
      setReadingLibraryVisible(false);
      setActiveTab('journeys');
      setSelectedJourneyId(result.journey.journey.id);
      getToday()
        .then((next) => {
          setToday(next);
          setTodayError(false);
        })
        .catch(() => setOffline(true));
      refreshCompanion();
    } catch (error) {
      setOffline(true);
      throw error;
    }
  };

  const handleJoinRoute = async (post: CommunityPost, context?: RecommendationEventContext) => {
    if (post.is_route === false) return;
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
      const result = await joinRoute(post.id, context);
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
    const fingerprint = JSON.stringify([journeyId, input]);
    if (creatingActionFingerprintsRef.current.has(fingerprint)) return;
    creatingActionFingerprintsRef.current.add(fingerprint);
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
      .catch(() => {
        // Keep the retry key in the API client, but remove this optimistic
        // placeholder so a retry cannot leave two local copies behind.
        setToday((current) => summariseToday(current.actions.filter((item) => item.id !== localAction.id)));
        setJourneyActionsById((current) => ({ ...current, [journeyId]: (current[journeyId] ?? []).filter((item) => item.id !== localAction.id) }));
        setOffline(true);
      })
      .finally(() => creatingActionFingerprintsRef.current.delete(fingerprint));
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

  const handleLike = (postId: string, context?: RecommendationEventContext) => {
    const active = !likedPostIds.has(postId);
    setLikedPostIds((current) => toggleId(current, postId, active));
    setFeed((current) => ({
      ...current,
      items: current.items.map((item) => {
        if (!item.post || item.post.id !== postId) return item;
        return { ...item, post: { ...item.post, like_count: Math.max(0, item.post.like_count + (active ? 1 : -1)) } };
      }),
    }));
    setFollowingFeed((current) => updateFeedLikeCount(current, postId, active));
    setContextualFeed((current) => current ? updateFeedLikeCount(current, postId, active) : current);
    setPostReaction(postId, 'like', active, undefined, context).catch(() => setOffline(true));
  };

  const handleBookmark = (postId: string, context?: RecommendationEventContext) => {
    const active = !bookmarkedPostIds.has(postId);
    setBookmarkedPostIds((current) => toggleId(current, postId, active));
    setPostReaction(postId, 'bookmark', active, undefined, context).catch(() => setOffline(true));
  };

  const handleCapturePostAsKnowledge = async (postId: string, context?: RecommendationEventContext) => {
    try {
      const resource = await capturePostAsKnowledge(postId, context);
      setKnowledgeResources((current) => upsertById(current, resource));
      setBookmarkedPostIds((current) => toggleId(current, postId, true));
    } catch (error) {
      setOffline(true);
      throw error;
    }
  };

  const handleHide = (
    postId: string,
    context: RecommendationEventContext | undefined,
    reason: NegativeFeedbackReason,
  ) => {
    setFeed((current) => ({
      ...current,
      items: current.items.filter((item) => item.post?.id !== postId),
    }));
    setFollowingFeed((current) => ({
      ...current,
      items: current.items.filter((item) => item.post?.id !== postId),
    }));
    setContextualFeed((current) => current ? {
      ...current,
      items: current.items.filter((item) => item.post?.id !== postId),
    } : current);
    setSelectedPost((current) => current?.id === postId ? undefined : current);
    setSelectedContent((current) => current?.id === postId ? undefined : current);
    setPostReaction(postId, 'hide', true, reason, context).catch(() => setOffline(true));
  };

  const requestHide = (postId: string, context?: RecommendationEventContext) => {
    setHideFeedback({ postId, context });
  };

  const submitHideFeedback = (reason: NegativeFeedbackReason) => {
    const feedback = hideFeedback;
    if (!feedback) return;
    setHideFeedback(undefined);
    handleHide(feedback.postId, feedback.context, reason);
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
    if (!item.post) return;
    trackFeedEvent('click', item.post.id, item.recommendation_context);
    openPostDetail(item.post, item.author_id || undefined, undefined, item.recommendation_context);
  };

  const clearContextualFeed = () => {
    contextualFeedRequestRef.current += 1;
    contextualFeedLoadingMoreRef.current = false;
    setContextualFeed(undefined);
    setContextualFeedContext(undefined);
    setContextualFeedLoading(false);
    setContextualOffers([]);
    setContextualOffersLoading(false);
    setContextualOffersError(false);
    setContextualAction(undefined);
    setContextualResources([]);
    setContextualResourcesLoading(false);
    setContextualResourcesError(false);
  };

  const closePostDetail = () => {
    postDetailRequestRef.current += 1;
    setSelectedPost(undefined);
    setSelectedPostAuthorId(undefined);
    setSelectedContent(undefined);
    setSelectedPostRecommendationContext(undefined);
  };

  const changeTab = (tab: TabKey) => {
    if (tab !== 'discover' && contextualFeedContext) clearContextualFeed();
    setActiveTab(tab);
  };

  const openActionContext = (
    routeId: string,
    action: RouteTemplateAction,
    sceneEquipment: string,
  ) => {
    const context: FeedActionContext = {
      route_id: routeId,
      action_node_id: action.id,
      placement: 'action_node',
      action_domain: selectedPost?.id === routeId ? selectedPost?.domain : undefined,
      scene_equipment: sceneEquipment,
    };
    const requestId = ++contextualFeedRequestRef.current;
    contextualFeedLoadingMoreRef.current = false;
    setContextualFeedContext(context);
    setContextualAction(action);
    setContextualOffers([]);
    setContextualOffersError(false);
    setContextualOffersLoading(true);
    setContextualResources([]);
    setContextualResourcesError(false);
    setContextualResourcesLoading(true);
    setContextualFeed({
      request_id: `contextual-${requestId}`,
      items: [],
      meta: { sourced: 0, filtered: 0, selected: 0 },
    });
    setContextualFeedLoading(true);
    setActiveTab('discover');
    closePostDetail();
    getFeed(undefined, undefined, 'home', context)
      .then((next) => {
        if (requestId !== contextualFeedRequestRef.current) return;
        setContextualFeed(attachFeedAttribution(next, 'home'));
      })
      .catch(() => {
        if (requestId !== contextualFeedRequestRef.current) return;
        setContextualFeed({
          request_id: `contextual-${requestId}`,
          items: [],
          meta: { sourced: 0, filtered: 0, selected: 0, degraded: true },
        });
        setOffline(true);
      })
      .finally(() => {
        if (requestId === contextualFeedRequestRef.current) setContextualFeedLoading(false);
      });
    getRouteNodeOffers(routeId, action.id)
      .then((response) => {
        if (requestId !== contextualFeedRequestRef.current) return;
        setContextualOffers(response.items);
      })
      .catch(() => {
        if (requestId !== contextualFeedRequestRef.current) return;
        setContextualOffersError(true);
      })
      .finally(() => {
        if (requestId === contextualFeedRequestRef.current) setContextualOffersLoading(false);
      });
    getRouteNodeResources(routeId, action.id)
      .then((response) => {
        if (requestId !== contextualFeedRequestRef.current) return;
        setContextualResources(response.items);
      })
      .catch(() => {
        if (requestId !== contextualFeedRequestRef.current) return;
        setContextualResourcesError(true);
      })
      .finally(() => {
        if (requestId === contextualFeedRequestRef.current) setContextualResourcesLoading(false);
      });
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

  const handleFollowAuthor = (authorId: string) => {
    if (!authorId || authorId === currentUserId) return;
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
        // Social graph is the ranking source of truth; this records the successful
        // user decision for aggregate product analysis without treating an author as content.
        if (active) eventReporter.track({ event_type: 'follow', component_id: 'creator-follow' });
        getFeed(undefined, undefined, 'following')
          .then((next) => setFollowingFeed(attachFeedAttribution(next, 'following')))
          .catch(() => setOffline(true));
      })
      .catch(() => {
        setFollowingAuthorIds((current) => toggleId(current, authorId, !active));
        setFollowingFeed(previousFollowingFeed);
        setOffline(true);
      });
  };

  const handleCreatorRelationship = (authorId: string, edge: 'mute' | 'block', active: boolean) => {
    if (!authorId || authorId === currentUserId) return;
    const setRelationship = edge === 'mute' ? setMutedAuthorIds : setBlockedAuthorIds;
    const previousFeed = feed;
    const previousFollowingFeed = followingFeed;
    setRelationship((current) => toggleId(current, authorId, active));
    if (active) {
      setFeed((current) => ({ ...current, items: current.items.filter((item) => item.author_id !== authorId) }));
      setFollowingFeed((current) => ({ ...current, items: current.items.filter((item) => item.author_id !== authorId) }));
      if (edge === 'block') closeAuthorProfile();
    }
    setCreatorRelationship(authorId, edge, active)
      .then((context) => {
        setFollowingAuthorIds(new Set(context.followed_author_ids));
        setMutedAuthorIds(new Set(context.muted_author_ids));
        setBlockedAuthorIds(new Set(context.blocked_author_ids));
      })
      .catch(() => {
        setRelationship((current) => toggleId(current, authorId, !active));
        setFeed(previousFeed);
        setFollowingFeed(previousFollowingFeed);
        setOffline(true);
      });
  };

  const handleFollow = (post: CommunityPost) => {
    const authorId = selectedPost?.id === post.id ? selectedPostAuthorId : undefined;
    if (authorId) handleFollowAuthor(authorId);
  };

  const handleComment = async (postId: string, body: string, parentId?: string) => {
    const localComment: Comment = {
      id: `local-comment-${Date.now()}`,
      post_id: postId,
      author_id: currentUserId ?? '',
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

  const handleAcceptQuestionAnswer = async (postId: string, commentId: string) => {
    const content = await acceptQuestionAnswer(postId, commentId);
    setSelectedContent((current) => current?.id === postId ? content : current);
    return content;
  };

  const handleSaveEntry = (input: CreateEntryInput) => {
    const fingerprint = JSON.stringify(input);
    if (creatingEntryFingerprintsRef.current.has(fingerprint)) return;
    creatingEntryFingerprintsRef.current.add(fingerprint);
    const entry: GrowthEntry = {
      ...input,
      id: `entry-${Date.now()}`,
      created_at: new Date().toISOString(),
      published: false,
      publication_status: input.published ? 'pending' : 'private',
    };
    setEntries((current) => [entry, ...current]);
    setEntryContext(null);
    createEntry(input)
      .then((saved) => {
        setEntries((current) => current.map((item) => item.id === entry.id ? saved : item));
        getWeeklyReview().then(setWeeklyReview).catch(() => setOffline(true));
      })
      .catch(() => {
        setEntries((current) => current.filter((item) => item.id !== entry.id));
        setOffline(true);
      })
      .finally(() => creatingEntryFingerprintsRef.current.delete(fingerprint));
  };

  const handleRetryEntryPublication = async (entryId: string) => {
    if (retryingEntryIds.has(entryId)) return;
    setRetryingEntryIds((current) => new Set(current).add(entryId));
    try {
      const entry = await retryEntryPublication(entryId);
      setEntries((current) => current.map((item) => item.id === entry.id ? entry : item));
    } catch {
      setOffline(true);
    } finally {
      setRetryingEntryIds((current) => {
        const next = new Set(current);
        next.delete(entryId);
        return next;
      });
    }
  };

  const handlePublishJourney = (journey: Journey, actions: Action[]) => {
    const routeTemplate = actions.length > 0 ? {
      intent: journey.intent.trim() || journey.title,
      completion_criteria: journey.completion_criteria.trim() || '完成路线中的必要阶段和行动',
      journey_type: journey.journey_type,
      // Public stages get their own stable ids minted at publish time. The
      // private journey's stage ids stay private, and the published ids do not
      // move when the author later reorders the public route.
      stages: journey.stages.map(({ title, detail, completion_criteria }, position) => ({
        id: `stage-${position + 1}`,
        title: title.trim(),
        detail: detail.trim(),
        completion_criteria: completion_criteria.trim(),
      })),
      actions: actions.map((action) => {
        const stageIndex = journey.stages.findIndex((stage) => stage.id === action.stage_id);
        return {
          id: action.id,
          title: action.title.trim(),
          detail: action.detail.trim(),
          estimated_minutes: action.estimated_minutes,
          // A public route is a reusable method, never the author's exact calendar.
          scheduled_label: '按自己的节奏安排',
          ...(stageIndex >= 0 ? { stage_id: `stage-${stageIndex + 1}` } : {}),
        };
      }),
    } : undefined;
    submitPostForReview({
      title: journey.title,
      summary: journey.intent,
      body: `${journey.intent}\n\n我正在走这条路线，欢迎根据自己的节奏调整。`,
      domain: journey.domain,
      content_type: 'route',
      tags: [],
      topics: [],
      route_title: journey.title,
      route_duration: journey.duration_label,
      route_template: routeTemplate,
    }).catch(() => setOffline(true));
  };

  const handleCreatePost = async (input: CreatePostInput) => {
    try {
      await submitPostForReview(input);
      setCreatingPost(false);
    } catch (error) {
      setOffline(true);
      throw error;
    }
  };

  const openJourney = (journey: Journey) => {
    setSelectedJourneyId(journey.id);
    getJourney(journey.id)
      .then((detail) => setJourneyActionsById((current) => ({ ...current, [journey.id]: detail.actions })))
      .catch(() => setOffline(true));
  };

  const screen = {
    today: <TodayScreen companion={companion} error={todayError} journeys={journeys} loading={initialLoading} notificationCount={notificationPage.unread_count} offline={offline} onComplete={handleComplete} onCreateJourney={() => setCreatingJourney(true)} onDiscover={() => setActiveTab('discover')} onNotifications={openNotifications} onOpenAction={openAction} today={today} />,
    discover: <DiscoverScreen bookmarkedPostIds={bookmarkedPostIds} contextualAction={contextualAction} contextualFeed={contextualFeed} contextualFeedContext={contextualFeedContext} contextualFeedLoading={contextualFeedLoading} contextualOffers={contextualOffers} contextualOffersError={contextualOffersError} contextualOffersLoading={contextualOffersLoading} contextualResources={contextualResources} contextualResourcesError={contextualResourcesError} contextualResourcesLoading={contextualResourcesLoading} feed={feed} feedError={feedError} feedLoadingMore={feedLoadingMore} followingFeed={followingFeed} followingFeedError={followingFeedError} followingFeedLoadingMore={followingFeedLoadingMore} initialLoading={initialLoading} joinedRouteIds={joinedRouteIds} joiningRouteIds={joiningRouteIds} likedPostIds={likedPostIds} offline={offline} onBookmark={handleBookmark} onClearContextualFeed={clearContextualFeed} onCreateOrder={(offerId, items, adAttribution) => createMallOrder(offerId, items, adAttribution).then((order) => { refreshMyOrders(); return order; })} onPayOrder={(id, ref) => payMallOrder(id, ref).then((order) => { refreshMyOrders(); return order; })} onHide={requestHide} onJoin={handleJoinRoute} onLike={handleLike} onLoadMoreFeed={loadMoreFeed} onOpen={openPost} onOpenAuthor={openAuthorProfile} routeParticipantCounts={routeParticipantCounts} />,
    journeys: <JourneysScreen error={journeysError} journeys={journeys} loading={initialLoading} offline={offline} onCreate={() => setCreatingJourney(true)} onOpen={openJourney} />,
    profile: <ProfileScreen entries={entries} entriesError={entriesError} journeys={journeys} journeysError={journeysError} loading={initialLoading} moderator={canModerate} offline={offline} onOpenFeedback={() => setFeedbackVisible(true)} onOpenLibrary={() => setReadingLibraryVisible(true)} onOpenMessages={() => openMessages()} onOpenModeration={() => setModerationVisible(true)} onOpenPublicResources={() => setResourceCatalogVisible(true)} onOpenSection={setProfileSection} onRetryOrders={refreshMyOrders} orders={myOrders} ordersError={ordersError} ordersLoading={ordersLoading} profile={profile} today={today} todayError={todayError} />,
  }[activeTab];

  return (
    <SafeAreaProvider>
      <SafeAreaView edges={['top', 'left', 'right']} style={styles.safeArea}>
        <View style={styles.screen}>{screen}</View>
        <TabBar active={activeTab} onChange={changeTab} onCreate={() => setCreateMenuVisible(true)} />
      </SafeAreaView>
      <CreateMenuModal onClose={() => setCreateMenuVisible(false)} onCreateEntry={() => { setCreateMenuVisible(false); setEntryContext({}); }} onCreateJourney={() => { setCreateMenuVisible(false); setCreatingJourney(true); }} onCreatePost={() => { setCreateMenuVisible(false); setCreatingPost(true); }} visible={createMenuVisible} />
      <CreateJourneyModal onClose={() => setCreatingJourney(false)} onSubmit={handleCreateJourney} visible={creatingJourney} />
      <CreatePostModal onClose={() => setCreatingPost(false)} onSubmit={handleCreatePost} visible={creatingPost} />
      <CreateEntryModal actionId={entryContext?.actionId} initialDurationMinutes={entryContext?.durationMinutes} journeyId={entryContext?.journeyId} journeys={journeys} onClose={() => setEntryContext(null)} onSubmit={handleSaveEntry} visible={entryContext !== null} />
      <ActionDetailModal action={selectedAction} journeyTitle={journeys.find((journey) => journey.id === selectedAction?.journey_id)?.title} onClose={() => setSelectedActionId(undefined)} onComplete={handleComplete} onCreateEntry={(action, elapsedSeconds) => { setSelectedActionId(undefined); setEntryContext({ actionId: action.id, journeyId: action.journey_id, durationMinutes: elapsedSeconds > 0 ? Math.max(1, Math.round(elapsedSeconds / 60)) : undefined }); }} onUpdate={handleUpdateAction} visible={Boolean(selectedAction)} />
      <JourneyDetailModal actions={selectedJourneyActions} journey={selectedJourney} onAddAction={handleAddAction} onClose={() => setSelectedJourneyId(undefined)} onOpenAction={openAction} onPublish={handlePublishJourney} onUpdateJourney={handleUpdateJourney} visible={Boolean(selectedJourney)} />
      <AuthorProfileModal author={selectedAuthor} blocked={Boolean(selectedAuthor && blockedAuthorIds.has(selectedAuthor.id))} contents={authorContents} error={authorContentsError} following={Boolean(selectedAuthor && followingAuthorIds.has(selectedAuthor.id))} loading={authorContentsLoading} loadingMore={authorContentsLoadingMore} muted={Boolean(selectedAuthor && mutedAuthorIds.has(selectedAuthor.id))} nextCursor={authorContentsNextCursor} onClose={closeAuthorProfile} onFollow={handleFollowAuthor} onLoadMore={loadMoreAuthorContents} onMessage={(authorId) => { closeAuthorProfile(); openMessages(authorId); }} onOpenContent={(content) => { if (!content.post) return; closeAuthorProfile(); openPostDetail(content.post, content.author_id, content); }} onSetRelationship={handleCreatorRelationship} stats={authorStats} />
      <PostDetailModal bookmarked={Boolean(selectedPost && bookmarkedPostIds.has(selectedPost.id))} canForkRoute={Boolean(selectedPost && selectedPostAuthorId && selectedPostAuthorId !== currentUserId)} capturedKnowledge={Boolean(selectedPost && knowledgeResources.some((resource) => resource.source_content_id === selectedPost.id))} comments={selectedPost ? commentsByPost[selectedPost.id] ?? [] : []} content={selectedContent} currentUserId={currentUserId} following={Boolean(selectedPostAuthorId && followingAuthorIds.has(selectedPostAuthorId))} hasMoreComments={Boolean(selectedPost && commentNextCursorByPost[selectedPost.id])} joinCount={selectedPost ? routeParticipantCounts[selectedPost.id] : undefined} joined={Boolean(selectedPost && joinedRouteIds.has(selectedPost.id))} joining={Boolean(selectedPost && joiningRouteIds.has(selectedPost.id))} liked={Boolean(selectedPost && likedPostIds.has(selectedPost.id))} loadingMoreComments={Boolean(selectedPost && loadingCommentPostIds.has(selectedPost.id))} onAcceptAnswer={handleAcceptQuestionAnswer} onBookmark={(postId) => handleBookmark(postId, selectedPostRecommendationContext)} onCaptureKnowledge={(postId) => handleCapturePostAsKnowledge(postId, selectedPostRecommendationContext)} onClose={closePostDetail} onComment={handleComment} onDeleteComment={handleDeleteComment} onFollow={handleFollow} onForkRoute={openForkRoute} onHide={(postId) => requestHide(postId, selectedPostRecommendationContext)} onJoin={(post) => handleJoinRoute(post, selectedPostRecommendationContext)} onLike={(postId) => handleLike(postId, selectedPostRecommendationContext)} onLoadMoreComments={handleLoadMoreComments} onOpenActionContext={openActionContext} onOpenAuthor={openPostAuthorProfile} onReport={handleReport} post={selectedPost} visible={Boolean(selectedPost)} />
      <ForkRouteModal onClose={() => setForkSourcePost(undefined)} onSubmit={handleForkRoute} sourceTitle={forkSourcePost?.route_title || forkSourcePost?.title || ''} visible={Boolean(forkSourcePost)} />
      <RouteDraftModal content={selectedRouteDraft} onClose={() => setSelectedRouteDraft(undefined)} onPublish={publishRouteDraft} onSave={saveRouteDraft} visible={Boolean(selectedRouteDraft)} />
      <HideFeedbackModal onClose={() => setHideFeedback(undefined)} onSelect={submitHideFeedback} visible={Boolean(hideFeedback)} />
      <ProfileSectionModal entries={entries} journeys={journeys} onApplyReviewSuggestion={handleApplyReviewSuggestion} onClose={() => setProfileSection(undefined)} onOpenRouteDraft={openRouteDraft} onRetryEntryPublication={handleRetryEntryPublication} retryingEntryIds={retryingEntryIds} review={weeklyReview} savedPosts={savedPosts} section={profileSection} visible={Boolean(profileSection)} />
      <FeedbackModal onClose={() => setFeedbackVisible(false)} visible={feedbackVisible} />
      <ModerationCommentsModal onClose={() => setModerationVisible(false)} visible={moderationVisible} />
      <NotificationsModal failedNotificationId={failedNotificationId} loading={notificationsLoading} loadingMore={notificationsLoadingMore} nextCursor={notificationPage.next_cursor} notifications={notificationPage.items} onClose={() => { notificationOpenRequestRef.current += 1; setOpeningNotificationId(undefined); setNotificationsVisible(false); }} onLoadMore={() => void loadMoreNotifications()} onOpenNotification={openNotification} onRefresh={() => void refreshNotifications()} openingNotificationId={openingNotificationId} unreadCount={notificationPage.unread_count} visible={notificationsVisible} />
      <MessagesModal initialRecipientId={messageRecipientId} onClose={() => { setMessagesVisible(false); setMessageRecipientId(undefined); }} visible={messagesVisible} />
      <ResourceCatalogModal onClose={() => setResourceCatalogVisible(false)} visible={resourceCatalogVisible} />
      <ReadingLibraryModal bookmarks={readingBookmarks} books={readingBooks} onClose={() => setReadingLibraryVisible(false)} onCreateResource={handleCreateKnowledgeResource} onOpenBook={(book) => openReader(book)} onStartJourney={handleStartKnowledgeJourney} onUpdateResource={handleUpdateKnowledgeResource} resources={knowledgeResources} visible={readingLibraryVisible} />
      <ReaderModal bookmarks={readingBookmarks} book={activeReadingBook} linkedAction={linkedReadingAction} onClose={closeReader} onCompleteAction={handleComplete} onSaveProgress={handleSaveReadingProgress} onToggleBookmark={handleToggleReadingBookmark} onUpdateSettings={(updates) => setReaderSettings((current) => ({ ...current, ...updates }))} settings={readerSettings} visible={Boolean(activeReadingBook)} />
      <StatusBar style={activeReadingBook && readerSettings.theme === 'night' ? 'light' : 'dark'} />
    </SafeAreaProvider>
  );
}

function trackFeedEvent(
  eventType: 'click' | 'view',
  contentId: string,
  context?: RecommendationEventContext,
) {
  const component = {
    click: 'open',
    view: 'detail',
  }[eventType];
  const prefix = context?.surface === 'search' ? 'search' : 'feed';
  eventReporter.track({
    event_type: eventType,
    component_id: `${prefix}-${component}`,
    content_id: contentId,
    position: context?.position,
    request_id: context?.request_id,
    attribution_source: context?.request_id ? context.surface === 'search' ? 'search' : 'recommendation' : undefined,
    source: context?.surface === 'search' ? 'mobile-search' : 'mobile',
  });
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

function upsertById<T extends { id: string }>(items: T[], item: T) {
  return items.some((current) => current.id === item.id)
    ? items.map((current) => current.id === item.id ? item : current)
    : [item, ...items];
}

function mergeFeedPages(current: Feed, incoming: Feed): Feed {
  const knownIds = new Set(current.items.map(feedItemKey));
  const items = [...current.items, ...incoming.items.filter((item) => !knownIds.has(feedItemKey(item)))];
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

function entryPublicationStatus(entry: GrowthEntry): 'private' | 'pending' | 'reviewing' | 'published' | 'restricted' | 'failed' {
  const status = entry.publication_status;
  if (status === 'pending') return 'pending';
  if (status === 'reviewing') return 'reviewing';
  if (status === 'published' || entry.published) return 'published';
  if (status === 'restricted') return 'restricted';
  if (status === 'failed') return 'failed';
  return 'private';
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

function journeyFromKnowledgeResource(resource: KnowledgeResource): CreateJourneyInput {
  const resourceTitle = resource.title.trim() || '这条灵感';
  const shortTitle = resourceTitle.slice(0, 80);
  const summary = resource.summary.trim();
  return {
    title: `实践：${shortTitle}`,
    intent: summary || `从「${shortTitle}」中找到一个可以亲自验证的改变。`,
    domain: 'learning',
    journey_type: 'project',
    completion_criteria: '完成第一次行动，并写下一条自己的观察。',
    stages: [],
    duration_label: '一周',
    first_action_title: `提炼「${shortTitle}」的一个行动`,
    first_action_detail: `花 20 分钟回看这条${knowledgeKindNoun(resource.kind)}，只记下一件愿意在今天尝试的事。`,
    estimated_minutes: 20,
    first_action_scheduled_label: '今天',
  };
}

function knowledgeKindNoun(kind: KnowledgeResource['kind']) {
  return ({ article: '文章', book: '书籍', course: '课程', video: '视频', link: '内容', note: '笔记' })[kind];
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
    items: current.items.map((item) => {
      if (!item.post || item.post.id !== postId) return item;
      return { ...item, post: { ...item.post, like_count: Math.max(0, item.post.like_count + (active ? 1 : -1)) } };
    }),
  };
}

function feedItemKey(item: FeedItem): string {
  if (item.ad) return `ad:${item.ad.request_id}:${item.ad.campaign_id}`;
  return `post:${item.post?.id ?? item.source}`;
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
