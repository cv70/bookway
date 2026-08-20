import { BookOpen, Check, Clock3, ExternalLink, Search, ShoppingBag, X } from 'lucide-react-native';
import { useEffect, useRef, useState } from 'react';
import {
  ActivityIndicator,
  Linking,
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  TextInput,
  View,
} from 'react-native';

import { ApiRequestError, getSuggestions, search as searchApi } from '../api/client';
import { eventReporter } from '../analytics/eventReporter';
import { FeedCard } from '../components/FeedCard';
import { ScreenHeader } from '../components/ScreenHeader';
import { colors, domainMeta } from '../theme';
import { Feed, FeedActionContext, FeedItem, GrowthDomain, MallOrder, NodeOffer, PublicAuthor, RouteNodeResourceAttachment, RouteTemplateAction, SearchResponse, SearchResult, SuggestionResponse } from '../types';
import { attachSearchAttribution, searchAttribution } from '../utils/feedAttribution';

type Filter = 'all' | GrowthDomain;
type FeedMode = 'recommend' | 'following';
type FeedSurface = 'home' | 'following';

const filters: Array<{ key: Filter; label: string }> = [
  { key: 'all', label: '为你推荐' },
  ...Object.entries(domainMeta).map(([key, value]) => ({ key: key as GrowthDomain, label: value.label })),
];

export function DiscoverScreen({
  feed,
  contextualAction,
  contextualFeedContext,
  contextualFeed,
  contextualFeedLoading = false,
  contextualOffers = [],
  contextualOffersError = false,
  contextualOffersLoading = false,
  contextualResources = [],
  contextualResourcesError = false,
  contextualResourcesLoading = false,
  followingFeed,
  feedLoadingMore = false,
  followingFeedLoadingMore = false,
  likedPostIds,
  bookmarkedPostIds,
  joinedRouteIds,
  joiningRouteIds,
  routeParticipantCounts,
  offline = false,
  onLike,
  onBookmark,
  onCreateOrder,
  onHide,
  onJoin,
  onLoadMoreFeed,
  onClearContextualFeed,
  onOpen,
  onOpenAuthor,
}: {
  feed: Feed;
  contextualAction?: RouteTemplateAction;
  contextualFeedContext?: FeedActionContext;
  contextualFeed?: Feed;
  contextualFeedLoading?: boolean;
  contextualOffers?: NodeOffer[];
  contextualOffersError?: boolean;
  contextualOffersLoading?: boolean;
  contextualResources?: RouteNodeResourceAttachment[];
  contextualResourcesError?: boolean;
  contextualResourcesLoading?: boolean;
  followingFeed: Feed;
  feedLoadingMore?: boolean;
  followingFeedLoadingMore?: boolean;
  likedPostIds?: Set<string>;
  bookmarkedPostIds?: Set<string>;
  joinedRouteIds?: Set<string>;
  joiningRouteIds?: Set<string>;
  routeParticipantCounts?: Record<string, number>;
  offline?: boolean;
  onLike?: (postId: string, context?: FeedItem['recommendation_context']) => void;
  onBookmark?: (postId: string, context?: FeedItem['recommendation_context']) => void;
  onCreateOrder?: (nodeOfferId: string, items: Array<{ sku_id: string; quantity: number }>) => Promise<MallOrder>;
  onHide?: (postId: string, context?: FeedItem['recommendation_context']) => void;
  onJoin?: (post: NonNullable<Feed['items'][number]['post']>, context?: FeedItem['recommendation_context']) => void;
  onLoadMoreFeed?: (surface: FeedSurface) => void;
  onClearContextualFeed?: () => void;
  onOpen?: (item: Feed['items'][number]) => void;
  onOpenAuthor?: (author: PublicAuthor) => void;
}) {
  const [filter, setFilter] = useState<Filter>('all');
  const [mode, setMode] = useState<FeedMode>('recommend');
  const [query, setQuery] = useState('');
  const [searchResponse, setSearchResponse] = useState<SearchResponse | null>(null);
  const [suggestions, setSuggestions] = useState<SuggestionResponse['items']>([]);
  const [recentQueries, setRecentQueries] = useState<string[]>([]);
  const [searching, setSearching] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [orderingOfferId, setOrderingOfferId] = useState<string>();
  const [orderNotice, setOrderNotice] = useState<string>();
  const searchRequestId = useRef(0);
  const suggestionRequestId = useRef(0);
  const loadingMoreRef = useRef(false);
  const contextualActive = mode === 'recommend' && Boolean(contextualFeed || contextualFeedLoading);
  const activeFeed = mode === 'following' ? followingFeed : contextualFeed ?? feed;
  const visibleFeed = activeFeed.items;
  const feedLoading = mode === 'following' ? followingFeedLoadingMore : contextualFeedLoading || feedLoadingMore;
  const items = filter === 'all' ? visibleFeed : visibleFeed.filter((item) => item.ad || item.post?.domain === filter);
  const searchResults = searchResponse?.items ?? null;
  const visibleSearchResults = (searchResults?.map((result, position) => ({ result, attribution: result.event_context ?? searchAttribution(position, searchResponse?.request_id) })) ?? null)
    ?.filter(({ result }) => filter === 'all' || (result.post?.domain ?? result.domain) === filter);

  useEffect(() => {
    setOrderingOfferId(undefined);
    setOrderNotice(undefined);
  }, [contextualAction?.id, contextualFeedContext?.route_id, contextualFeedContext?.action_node_id]);

  const contextualOfferItems = contextualFeedContext
    ? contextualOffers.filter((offer) => (
      offer.route_id === contextualFeedContext.route_id
      && offer.action_node_id === contextualFeedContext.action_node_id
      && offer.scene_equipment === contextualFeedContext.scene_equipment
    ))
    : [];
  const contextualResourceItems = contextualFeedContext
    ? contextualResources.filter((attachment) => (
      attachment.route_id === contextualFeedContext.route_id
      && attachment.action_node_id === contextualFeedContext.action_node_id
    ))
    : [];

  const createOrder = async (offer: NodeOffer) => {
    const sku = offer.product?.skus.find((item) => item.id === offer.sku_id && item.saleable);
    if (!onCreateOrder || !sku || orderingOfferId) return;
    setOrderingOfferId(offer.id);
    setOrderNotice(undefined);
    try {
      const order = await onCreateOrder(offer.id, [{ sku_id: sku.id, quantity: 1 }]);
      setOrderNotice(`订单已创建 · ${order.id}`);
    } catch {
      setOrderNotice('暂时无法创建订单，请稍后重试');
    } finally {
      setOrderingOfferId(undefined);
    }
  };

  const openResource = (attachment: RouteNodeResourceAttachment) => {
    const url = attachment.resource?.url?.trim();
    if (url?.startsWith('https://') || url?.startsWith('http://')) {
      Linking.openURL(url).catch(() => undefined);
    }
  };

  useEffect(() => {
    if (contextualFeed || contextualFeedLoading) setMode('recommend');
  }, [contextualFeed, contextualFeedLoading]);

  const openSearchResult = (result: SearchResult) => {
    if (result.result_type === 'user') {
      const authorId = result.author_id ?? result.id;
      if (authorId) onOpenAuthor?.({ id: authorId, name: result.author_name ?? result.title, avatar_url: result.cover_url });
      return;
    }
    if (result.result_type === 'topic') {
      setQuery(result.title);
      rememberQuery(result.title);
    }
  };

  useEffect(() => {
    const trimmed = query.trim();
    const requestId = ++searchRequestId.current;
    loadingMoreRef.current = false;
    setLoadingMore(false);
    if (!trimmed) {
      setSearchResponse(null);
      setSearching(false);
      return;
    }
    setSearchResponse(null);
    setSearching(true);
    const timer = setTimeout(() => {
      eventReporter.track({ event_type: 'search_submit', component_id: 'discover-search', source: 'mobile-search', content_id: undefined });
      searchApi(trimmed)
        .then((response) => {
          if (requestId !== searchRequestId.current) return;
          setRecentQueries((current) => [trimmed, ...current.filter((item) => item !== trimmed)].slice(0, 6));
          setSearchResponse(attachSearchAttribution(response));
        })
        .catch(() => {
          if (requestId !== searchRequestId.current) return;
          const lower = trimmed.toLowerCase();
          setSearchResponse({
            request_id: '',
            query: trimmed,
            items: feed.items
              .filter(({ post }) => post && `${post.title} ${post.summary} ${post.tags.join(' ')}`.toLowerCase().includes(lower))
              .flatMap(({ author_id, post, score }) => post ? [{
                id: post.id,
                result_type: 'post' as const,
                title: post.title,
                snippet: post.summary,
                cover_url: post.cover_url,
                author_id,
                author_name: post.author_name,
                domain: post.domain,
                score,
                highlights: [post.title],
                post,
              }] : []),
            total_estimate: feed.items.length,
            took_ms: 0,
            degraded: true,
          });
        })
        .finally(() => {
          if (requestId === searchRequestId.current) setSearching(false);
        });
    }, 260);
    return () => clearTimeout(timer);
  }, [feed.items, query]);

  useEffect(() => {
    const trimmed = query.trim();
    const requestId = ++suggestionRequestId.current;
    if (!trimmed) {
      setSuggestions([]);
      return;
    }
    const timer = setTimeout(() => {
      getSuggestions(trimmed)
        .then((response) => {
          if (requestId === suggestionRequestId.current) setSuggestions(response.items);
        })
        .catch(() => {
          if (requestId === suggestionRequestId.current) {
            setSuggestions(feed.items.flatMap((item) => item.post?.tags ?? []).filter((tag) => tag.includes(trimmed)).slice(0, 6).map((text) => ({ text, result_type: 'topic' as const, score: 0, personal: false })));
          }
        });
    }, 140);
    return () => clearTimeout(timer);
  }, [feed.items, query]);

  const rememberQuery = (value: string) => {
    const trimmed = value.trim();
    if (!trimmed) return;
    setRecentQueries((current) => [trimmed, ...current.filter((item) => item !== trimmed)].slice(0, 6));
  };

  const loadMoreSearch = () => {
    const trimmed = query.trim();
    const cursor = searchResponse?.next_cursor;
    if (!trimmed || searchResponse?.query !== trimmed || !cursor || loadingMoreRef.current) return;

    const requestId = searchRequestId.current;
    loadingMoreRef.current = true;
    setLoadingMore(true);
    searchApi(trimmed, cursor)
      .then((response) => {
        if (requestId !== searchRequestId.current) return;
        const attributedResponse = attachSearchAttribution(response);
        setSearchResponse((current) => {
          if (!current || current.query !== trimmed) return current;
          const seenIds = new Set(current.items.map((item) => item.id));
          return {
            ...attributedResponse,
            items: [...current.items, ...attributedResponse.items.filter((item) => !seenIds.has(item.id))],
            total_estimate: Math.max(current.total_estimate, attributedResponse.total_estimate),
            degraded: current.degraded || attributedResponse.degraded,
          };
        });
      })
      .catch((error: unknown) => {
        if (!(error instanceof ApiRequestError) || error.code !== 'failed_precondition') {
          // Retain the already-rendered page: an intermittent next-page failure is recoverable.
          return;
        }
        // PIT-backed cursors are deliberately short-lived. Restart from page one so a user
        // can continue searching instead of retrying an expired cursor forever.
        return searchApi(trimmed).then((response) => {
          if (requestId === searchRequestId.current) setSearchResponse(attachSearchAttribution(response));
        }).catch(() => {
          // Keep already rendered results if the refresh itself cannot be completed.
        });
      })
      .finally(() => {
        if (requestId === searchRequestId.current) {
          loadingMoreRef.current = false;
          setLoadingMore(false);
        }
      });
  };

  return (
    <ScrollView
      contentContainerStyle={styles.content}
      onScroll={({ nativeEvent }) => {
        if (nativeEvent.layoutMeasurement.height + nativeEvent.contentOffset.y < nativeEvent.contentSize.height - 180) return;
        if (query) loadMoreSearch();
        else if (!contextualFeedLoading) onLoadMoreFeed?.(mode === 'following' ? 'following' : 'home');
      }}
      scrollEventThrottle={200}
      showsVerticalScrollIndicator={false}
    >
      <ScreenHeader action={contextualActive ? 'close' : undefined} onAction={contextualActive ? onClearContextualFeed : undefined} title="发现" />
      {offline ? <Text style={styles.offline}>当前展示离线预览，连接恢复后会自动更新</Text> : null}
      <View style={styles.searchBox}>
        <Search color={colors.muted} size={18} />
        <TextInput
          accessibilityLabel="搜索万卷行"
          onChangeText={setQuery}
          placeholder="搜索行记、路线和主题"
          placeholderTextColor={colors.faint}
          returnKeyType="search"
          style={styles.searchInput}
          value={query}
        />
        {query ? (
          <Pressable accessibilityLabel="清空搜索" hitSlop={8} onPress={() => setQuery('')}>
            <X color={colors.muted} size={17} />
          </Pressable>
        ) : null}
      </View>
      {query ? (
        <View style={styles.searchState}>
          <Text style={styles.searchTitle}>搜索结果</Text>
          {searching ? <ActivityIndicator color={colors.evergreen} size="small" /> : null}
        </View>
      ) : null}
      {query && searchResults === null && suggestions.length ? <View style={styles.suggestions}>{suggestions.map((suggestion) => <Pressable key={`${suggestion.text}-${suggestion.result_type}`} onPress={() => { setQuery(suggestion.text); rememberQuery(suggestion.text); }} style={styles.suggestion}>{suggestion.personal ? <Clock3 color={colors.evergreen} size={15} /> : <Search color={colors.faint} size={15} />}<Text style={styles.suggestionText}>{suggestion.text}</Text></Pressable>)}</View> : null}
      {!query && recentQueries.length ? <View style={styles.recent}><View style={styles.recentHeader}><Text style={styles.recentTitle}>最近搜索</Text><Pressable accessibilityLabel="清除搜索历史" onPress={() => setRecentQueries([])}><X color={colors.faint} size={15} /></Pressable></View><View style={styles.recentItems}>{recentQueries.map((item) => <Pressable key={item} onPress={() => setQuery(item)} style={styles.recentItem}><Text style={styles.recentText}>{item}</Text></Pressable>)}</View></View> : null}
      {!query ? (
        <View style={styles.modeTabs}>
          {([['recommend', '推荐'], ['following', '关注']] as const).map(([key, label]) => {
            const selected = mode === key;
            return <Pressable accessibilityRole="tab" accessibilityState={{ selected }} key={key} onPress={() => setMode(key)} style={[styles.modeTab, selected && styles.modeTabSelected]}><Text style={[styles.modeText, selected && styles.modeTextSelected]}>{label}</Text></Pressable>;
          })}
        </View>
      ) : null}
      <ScrollView
        contentContainerStyle={styles.filters}
        horizontal
        showsHorizontalScrollIndicator={false}
      >
        {filters.map((item) => {
          const selected = filter === item.key;
          return (
            <Pressable
              accessibilityRole="tab"
              accessibilityState={{ selected }}
              key={item.key}
              onPress={() => setFilter(item.key)}
              style={[styles.filter, selected && styles.filterSelected]}
            >
              <Text style={[styles.filterText, selected && styles.filterTextSelected]}>{item.label}</Text>
            </Pressable>
          );
        })}
      </ScrollView>
      {contextualAction && contextualFeedContext ? (
        <View style={styles.contextPanel}>
          <View style={styles.contextHeader}>
            <View style={styles.contextHeading}>
              <ShoppingBag color={colors.evergreen} size={17} />
              <Text style={styles.contextTitle}>{contextualAction.title}</Text>
            </View>
            <Text style={styles.contextEquipment}>{contextualFeedContext.scene_equipment || '动作装备'}</Text>
          </View>
          <Text style={styles.contextDetail}>把这一步需要的装备带进路线，商品与当前动作节点绑定。</Text>
          {contextualOffersLoading ? (
            <View style={styles.offerState}><ActivityIndicator color={colors.evergreen} size="small" /><Text style={styles.offerStateText}>正在查找场景装备</Text></View>
          ) : contextualOffersError ? (
            <Text style={styles.offerStateText}>场景商品暂时不可用</Text>
          ) : contextualOfferItems.length ? (
            contextualOfferItems.map((offer) => {
              const product = offer.product;
              const sku = product?.skus.find((item) => item.id === offer.sku_id);
              const canOrder = Boolean(onCreateOrder && sku?.saleable);
              const price = sku ? `${sku.currency || 'CNY'} ${(sku.price_cents / 100).toFixed(2)}` : '价格待确认';
              return (
                <View key={offer.id} style={styles.offerRow}>
                  <View style={styles.offerCopy}>
                    <Text style={styles.offerTitle}>{product?.title || '场景装备'}</Text>
                    <Text style={styles.offerSku}>{sku?.title || offer.sku_id}</Text>
                    <Text style={styles.offerMeta}>{price} · 商家 {offer.merchant_id}</Text>
                  </View>
                  <Pressable
                    accessibilityLabel={canOrder ? `购买${product?.title || '场景装备'}` : '商品暂不可购买'}
                    accessibilityRole="button"
                    disabled={!canOrder || Boolean(orderingOfferId)}
                    onPress={() => void createOrder(offer)}
                    style={({ pressed }) => [styles.orderButton, (!canOrder || Boolean(orderingOfferId)) && styles.orderButtonDisabled, pressed && styles.pressed]}
                  >
                    {orderingOfferId === offer.id ? <ActivityIndicator color={colors.surface} size="small" /> : <ShoppingBag color={colors.surface} size={15} />}
                    <Text style={styles.orderButtonText}>{canOrder ? '创建订单' : '暂不可购'}</Text>
                  </Pressable>
                </View>
              );
            })
          ) : (
            <Text style={styles.offerStateText}>该动作暂未配置场景商品</Text>
          )}
          {orderNotice ? <View style={styles.orderNotice}><Check color={colors.evergreen} size={15} /><Text style={styles.orderNoticeText}>{orderNotice}</Text></View> : null}
          <View style={styles.resourcesSection}>
            <View style={styles.resourcesHeading}>
              <BookOpen color={colors.blue} size={16} />
              <Text style={styles.resourcesTitle}>行动资源</Text>
            </View>
            {contextualResourcesLoading ? (
              <View style={styles.offerState}><ActivityIndicator color={colors.blue} size="small" /><Text style={styles.offerStateText}>正在加载节点资源</Text></View>
            ) : contextualResourcesError ? (
              <Text style={styles.offerStateText}>节点资源暂时不可用</Text>
            ) : contextualResourceItems.length ? (
              contextualResourceItems.map((attachment) => {
                const resource = attachment.resource;
                const title = attachment.title_override.trim() || resource?.title || '行动资源';
                const url = resource?.url?.trim() ?? '';
                const canOpen = url.startsWith('https://') || url.startsWith('http://');
                return (
                  <Pressable
                    accessibilityLabel={canOpen ? `打开${title}` : title}
                    accessibilityRole="button"
                    disabled={!canOpen}
                    key={attachment.id}
                    onPress={() => openResource(attachment)}
                    style={({ pressed }) => [styles.resourceRow, !canOpen && styles.resourceRowDisabled, pressed && styles.pressed]}
                  >
                    <View style={styles.resourceCopy}>
                      <Text style={styles.resourceKind}>{resourceKindLabel(attachment.kind)}</Text>
                      <Text style={styles.resourceTitle}>{title}</Text>
                      <Text numberOfLines={2} style={styles.resourceDetail}>{attachment.note.trim() || resource?.summary || '为当前行动准备的参考资源'}</Text>
                    </View>
                    {canOpen ? <ExternalLink color={colors.blue} size={16} /> : null}
                  </Pressable>
                );
              })
            ) : (
              <Text style={styles.offerStateText}>该动作暂未挂载资源</Text>
            )}
          </View>
        </View>
      ) : null}
      <View>
        {query
          ? visibleSearchResults?.map(({ result, attribution }) =>
              result.post ? (
                <FeedCard
                  item={{ author_id: result.author_id ?? '', post: result.post, score: result.score, source: 'search', reasons: result.highlights, recommendation_context: attribution }}
                  bookmarked={bookmarkedPostIds?.has(result.id)}
                  key={result.id}
                  joined={joinedRouteIds?.has(result.id)}
                  joining={joiningRouteIds?.has(result.id)}
                  joinCount={routeParticipantCounts?.[result.id]}
                  liked={likedPostIds?.has(result.id)}
                  onBookmark={onBookmark}
                  onHide={onHide}
                  onJoin={onJoin}
                  onLike={onLike}
                  onOpen={onOpen}
                />
              ) : (
                <Pressable accessibilityLabel={result.result_type === 'user' ? `查看创作者${result.title}` : `搜索话题${result.title}`} key={result.id} onPress={() => openSearchResult(result)} style={({ pressed }) => [styles.resultRow, pressed && styles.pressed]}>
                  <Text style={styles.resultTitle}>{result.title}</Text>
                  <Text style={styles.resultSnippet}>{result.snippet}</Text>
                </Pressable>
              ),
            )
          : items.map((item) => item.ad ? (
              <FeedCard item={item} key={`ad:${item.ad.request_id}:${item.ad.campaign_id}`} />
            ) : item.post ? (
              <FeedCard
                item={item}
                bookmarked={bookmarkedPostIds?.has(item.post.id)}
                key={item.post.id}
                joined={joinedRouteIds?.has(item.post.id)}
                joining={joiningRouteIds?.has(item.post.id)}
                joinCount={routeParticipantCounts?.[item.post.id]}
                liked={likedPostIds?.has(item.post.id)}
                onBookmark={onBookmark}
                onHide={onHide}
                onJoin={onJoin}
                onLike={onLike}
                onOpen={onOpen}
              />
            ) : null)}
      </View>
      {query && searchResponse?.degraded ? <Text style={styles.degraded}>搜索服务暂不可用，已在当前已加载内容中查找</Text> : null}
      {query && searchResponse?.next_cursor ? (
        <Pressable accessibilityRole="button" disabled={loadingMore} onPress={loadMoreSearch} style={[styles.loadMore, loadingMore && styles.loadMoreDisabled]}>
          {loadingMore ? <ActivityIndicator color={colors.evergreen} size="small" /> : <Text style={styles.loadMoreText}>加载更多结果</Text>}
        </Pressable>
      ) : null}
      {!query && activeFeed.meta.next_cursor && !contextualFeedLoading ? (
        <Pressable accessibilityRole="button" disabled={feedLoading} onPress={() => onLoadMoreFeed?.(mode === 'following' ? 'following' : 'home')} style={[styles.loadMore, feedLoading && styles.loadMoreDisabled]}>
          {feedLoading ? <ActivityIndicator color={colors.evergreen} size="small" /> : <Text style={styles.loadMoreText}>加载更多{mode === 'following' ? '关注内容' : '推荐内容'}</Text>}
        </Pressable>
      ) : null}
      {((query && visibleSearchResults?.length === 0) || (!query && items.length === 0)) ? (
        <View style={styles.empty}><Text style={styles.emptyText}>{mode === 'following' && !query ? '关注创作者后，他们的行记会出现在这里' : '这一页还在生长'}</Text></View>
      ) : null}
    </ScrollView>
  );
}

function resourceKindLabel(kind: RouteNodeResourceAttachment['kind']) {
  return ({
    document: '文档',
    pdf: 'PDF',
    external_link: '链接',
    tool_checklist: '工具清单',
    ai_action_guide: '行动指南',
    rag_corpus: '参考资料',
  })[kind];
}

const styles = StyleSheet.create({
  content: { paddingBottom: 24 },
  searchBox: { height: 44, marginHorizontal: 16, marginBottom: 14, paddingHorizontal: 12, flexDirection: 'row', alignItems: 'center', gap: 8, borderWidth: 1, borderColor: colors.line, borderRadius: 7, backgroundColor: colors.surface },
  offline: { marginHorizontal: 16, marginBottom: 12, paddingHorizontal: 12, paddingVertical: 9, color: colors.muted, backgroundColor: colors.goldSoft, borderRadius: 6, fontSize: 12, lineHeight: 17 },
  searchInput: { flex: 1, minWidth: 0, color: colors.ink, fontSize: 14, paddingVertical: 0, letterSpacing: 0 },
  searchState: { height: 42, paddingHorizontal: 20, flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between' },
  searchTitle: { color: colors.ink, fontSize: 15, fontWeight: '700', letterSpacing: 0 },
  modeTabs: { height: 43, marginHorizontal: 16, marginBottom: 10, flexDirection: 'row', borderBottomWidth: StyleSheet.hairlineWidth, borderBottomColor: colors.line },
  modeTab: { flex: 1, alignItems: 'center', justifyContent: 'center', borderBottomWidth: 2, borderBottomColor: 'transparent' },
  modeTabSelected: { borderBottomColor: colors.evergreen },
  modeText: { color: colors.faint, fontSize: 13, fontWeight: '700', letterSpacing: 0 },
  modeTextSelected: { color: colors.evergreen },
  suggestions: { marginHorizontal: 16, marginBottom: 10, borderRadius: 6, backgroundColor: colors.surface, borderWidth: 1, borderColor: colors.line },
  suggestion: { minHeight: 40, paddingHorizontal: 12, flexDirection: 'row', alignItems: 'center', gap: 8, borderBottomWidth: StyleSheet.hairlineWidth, borderBottomColor: colors.line },
  suggestionText: { color: colors.ink, fontSize: 13, letterSpacing: 0 },
  recent: { marginHorizontal: 16, marginBottom: 12 },
  recentHeader: { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between' },
  recentTitle: { color: colors.muted, fontSize: 11, fontWeight: '700', letterSpacing: 0 },
  recentItems: { flexDirection: 'row', flexWrap: 'wrap', gap: 7, marginTop: 8 },
  recentItem: { minHeight: 30, paddingHorizontal: 10, alignItems: 'center', justifyContent: 'center', borderRadius: 5, backgroundColor: colors.surface, borderWidth: 1, borderColor: colors.line },
  recentText: { color: colors.muted, fontSize: 11, letterSpacing: 0 },
  filters: { paddingHorizontal: 16, gap: 7, paddingBottom: 14 },
  filter: { height: 34, justifyContent: 'center', paddingHorizontal: 13, borderRadius: 6, borderWidth: 1, borderColor: colors.line, backgroundColor: colors.surface },
  filterSelected: { backgroundColor: colors.ink, borderColor: colors.ink },
  filterText: { color: colors.muted, fontSize: 12, fontWeight: '600', letterSpacing: 0 },
  filterTextSelected: { color: colors.surface },
  contextPanel: { marginHorizontal: 16, marginBottom: 14, padding: 14, borderWidth: 1, borderColor: colors.evergreenSoft, borderRadius: 7, backgroundColor: colors.surface },
  contextHeader: { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', gap: 10 },
  contextHeading: { flex: 1, minWidth: 0, flexDirection: 'row', alignItems: 'center', gap: 7 },
  contextTitle: { flex: 1, color: colors.ink, fontSize: 15, fontWeight: '700', letterSpacing: 0 },
  contextEquipment: { maxWidth: '45%', color: colors.evergreen, fontSize: 12, fontWeight: '700', letterSpacing: 0 },
  contextDetail: { marginTop: 6, color: colors.muted, fontSize: 12, lineHeight: 17, letterSpacing: 0 },
  offerState: { minHeight: 42, flexDirection: 'row', alignItems: 'center', gap: 8 },
  offerStateText: { marginTop: 12, color: colors.muted, fontSize: 12, lineHeight: 17, letterSpacing: 0 },
  offerRow: { marginTop: 11, paddingTop: 11, flexDirection: 'row', alignItems: 'center', gap: 10, borderTopWidth: StyleSheet.hairlineWidth, borderTopColor: colors.line },
  offerCopy: { flex: 1, minWidth: 0 },
  offerTitle: { color: colors.ink, fontSize: 13, fontWeight: '700', letterSpacing: 0 },
  offerSku: { marginTop: 3, color: colors.muted, fontSize: 12, letterSpacing: 0 },
  offerMeta: { marginTop: 3, color: colors.faint, fontSize: 11, letterSpacing: 0 },
  orderButton: { minHeight: 35, paddingHorizontal: 10, flexDirection: 'row', alignItems: 'center', justifyContent: 'center', gap: 5, borderRadius: 6, backgroundColor: colors.evergreen },
  orderButtonDisabled: { opacity: 0.55 },
  orderButtonText: { color: colors.surface, fontSize: 12, fontWeight: '700', letterSpacing: 0 },
  orderNotice: { marginTop: 11, paddingTop: 10, flexDirection: 'row', alignItems: 'flex-start', gap: 6, borderTopWidth: StyleSheet.hairlineWidth, borderTopColor: colors.line },
  orderNoticeText: { flex: 1, color: colors.evergreen, fontSize: 12, lineHeight: 17, letterSpacing: 0 },
  resourcesSection: { marginTop: 13, paddingTop: 12, borderTopWidth: StyleSheet.hairlineWidth, borderTopColor: colors.line },
  resourcesHeading: { flexDirection: 'row', alignItems: 'center', gap: 7 },
  resourcesTitle: { color: colors.ink, fontSize: 13, fontWeight: '700', letterSpacing: 0 },
  resourceRow: { marginTop: 10, paddingTop: 10, flexDirection: 'row', alignItems: 'center', gap: 10, borderTopWidth: StyleSheet.hairlineWidth, borderTopColor: colors.line },
  resourceRowDisabled: { opacity: 0.65 },
  resourceCopy: { flex: 1, minWidth: 0 },
  resourceKind: { color: colors.blue, fontSize: 10, fontWeight: '700', letterSpacing: 0 },
  resourceTitle: { marginTop: 2, color: colors.ink, fontSize: 13, fontWeight: '700', letterSpacing: 0 },
  resourceDetail: { marginTop: 3, color: colors.muted, fontSize: 12, lineHeight: 17, letterSpacing: 0 },
  empty: { height: 200, alignItems: 'center', justifyContent: 'center' },
  emptyText: { color: colors.faint, fontSize: 14, letterSpacing: 0 },
  resultRow: { paddingHorizontal: 20, paddingVertical: 15, backgroundColor: colors.surface, borderBottomColor: colors.line, borderBottomWidth: StyleSheet.hairlineWidth },
  resultTitle: { color: colors.ink, fontSize: 15, fontWeight: '700', letterSpacing: 0 },
  resultSnippet: { color: colors.muted, fontSize: 13, lineHeight: 19, marginTop: 4, letterSpacing: 0 },
  degraded: { marginHorizontal: 20, marginTop: 12, color: colors.muted, fontSize: 12, lineHeight: 18, letterSpacing: 0 },
  loadMore: { minHeight: 42, marginHorizontal: 16, marginTop: 14, alignItems: 'center', justifyContent: 'center', borderRadius: 6, borderWidth: 1, borderColor: colors.line, backgroundColor: colors.surface },
  loadMoreDisabled: { opacity: 0.65 },
  loadMoreText: { color: colors.evergreen, fontSize: 13, fontWeight: '700', letterSpacing: 0 },
  pressed: { opacity: 0.62 },
});
