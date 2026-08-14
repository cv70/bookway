import { Search, X } from 'lucide-react-native';
import { useEffect, useState } from 'react';
import {
  ActivityIndicator,
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  TextInput,
  View,
} from 'react-native';

import { getSuggestions, search as searchApi } from '../api/client';
import { eventReporter } from '../analytics/eventReporter';
import { FeedCard } from '../components/FeedCard';
import { ScreenHeader } from '../components/ScreenHeader';
import { colors, domainMeta } from '../theme';
import { Feed, GrowthDomain, SearchResult } from '../types';

type Filter = 'all' | GrowthDomain;
type FeedMode = 'recommend' | 'following';

const filters: Array<{ key: Filter; label: string }> = [
  { key: 'all', label: '为你推荐' },
  ...Object.entries(domainMeta).map(([key, value]) => ({ key: key as GrowthDomain, label: value.label })),
];

export function DiscoverScreen({
  feed,
  likedPostIds,
  bookmarkedPostIds,
  joinedRouteIds,
  followingAuthorNames,
  offline = false,
  onLike,
  onBookmark,
  onJoin,
  onOpen,
}: {
  feed: Feed;
  likedPostIds?: Set<string>;
  bookmarkedPostIds?: Set<string>;
  joinedRouteIds?: Set<string>;
  followingAuthorNames?: Set<string>;
  offline?: boolean;
  onLike?: (postId: string) => void;
  onBookmark?: (postId: string) => void;
  onJoin?: (post: Feed['items'][number]['post']) => void;
  onOpen?: (post: Feed['items'][number]['post']) => void;
}) {
  const [filter, setFilter] = useState<Filter>('all');
  const [mode, setMode] = useState<FeedMode>('recommend');
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<SearchResult[] | null>(null);
  const [suggestions, setSuggestions] = useState<string[]>([]);
  const [recentQueries, setRecentQueries] = useState<string[]>([]);
  const [searching, setSearching] = useState(false);
  const visibleFeed = mode === 'following'
    ? feed.items.filter((item) => followingAuthorNames?.has(item.post.author_name))
    : feed.items;
  const items = filter === 'all' ? visibleFeed : visibleFeed.filter((item) => item.post.domain === filter);

  useEffect(() => {
    const trimmed = query.trim();
    if (!trimmed) {
      setResults(null);
      setSearching(false);
      return;
    }
    setSearching(true);
    const timer = setTimeout(() => {
      eventReporter.track({ event_type: 'search_submit', component_id: 'discover-search', source: 'mobile', content_id: undefined });
      searchApi(trimmed)
        .then((response) => {
          setRecentQueries((current) => [trimmed, ...current.filter((item) => item !== trimmed)].slice(0, 6));
          setResults(response.items);
        })
        .catch(() => {
          const lower = trimmed.toLowerCase();
          setResults(
            feed.items
              .filter(({ post }) => `${post.title} ${post.summary} ${post.tags.join(' ')}`.toLowerCase().includes(lower))
              .map(({ post, score }) => ({
                id: post.id,
                result_type: 'post' as const,
                title: post.title,
                snippet: post.summary,
                cover_url: post.cover_url,
                author_name: post.author_name,
                domain: post.domain,
                score,
                highlights: [post.title],
                post,
              })),
          );
        })
        .finally(() => setSearching(false));
    }, 260);
    return () => clearTimeout(timer);
  }, [feed, query]);

  useEffect(() => {
    const trimmed = query.trim();
    if (!trimmed) {
      setSuggestions([]);
      return;
    }
    const timer = setTimeout(() => {
      getSuggestions(trimmed)
        .then((response) => setSuggestions(response.items.map((item) => item.text)))
        .catch(() => setSuggestions(feed.items.flatMap((item) => item.post.tags).filter((tag) => tag.includes(trimmed)).slice(0, 6)));
    }, 140);
    return () => clearTimeout(timer);
  }, [feed.items, query]);

  const rememberQuery = (value: string) => {
    const trimmed = value.trim();
    if (!trimmed) return;
    setRecentQueries((current) => [trimmed, ...current.filter((item) => item !== trimmed)].slice(0, 6));
  };

  return (
    <ScrollView contentContainerStyle={styles.content} showsVerticalScrollIndicator={false}>
      <ScreenHeader title="发现" />
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
      {query && !searching && results === null && suggestions.length ? <View style={styles.suggestions}>{suggestions.map((suggestion) => <Pressable key={suggestion} onPress={() => { setQuery(suggestion); rememberQuery(suggestion); }} style={styles.suggestion}><Search color={colors.faint} size={15} /><Text style={styles.suggestionText}>{suggestion}</Text></Pressable>)}</View> : null}
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
      <View>
        {query
          ? results?.map((result) =>
              result.post ? (
                <FeedCard
                  item={{ post: result.post, score: result.score, source: 'search', reasons: result.highlights }}
                  bookmarked={bookmarkedPostIds?.has(result.id)}
                  key={result.id}
                  joined={joinedRouteIds?.has(result.id)}
                  liked={likedPostIds?.has(result.id)}
                  onBookmark={onBookmark}
                  onJoin={onJoin}
                  onLike={onLike}
                  onOpen={onOpen}
                />
              ) : (
                <View key={result.id} style={styles.resultRow}>
                  <Text style={styles.resultTitle}>{result.title}</Text>
                  <Text style={styles.resultSnippet}>{result.snippet}</Text>
                </View>
              ),
            )
          : items.map((item) => (
              <FeedCard
                item={item}
                bookmarked={bookmarkedPostIds?.has(item.post.id)}
                key={item.post.id}
                joined={joinedRouteIds?.has(item.post.id)}
                liked={likedPostIds?.has(item.post.id)}
                onBookmark={onBookmark}
                onJoin={onJoin}
                onLike={onLike}
                onOpen={onOpen}
              />
            ))}
      </View>
      {((query && results?.length === 0) || (!query && items.length === 0)) ? (
        <View style={styles.empty}><Text style={styles.emptyText}>{mode === 'following' && !query ? '关注创作者后，他们的行记会出现在这里' : '这一页还在生长'}</Text></View>
      ) : null}
    </ScrollView>
  );
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
  empty: { height: 200, alignItems: 'center', justifyContent: 'center' },
  emptyText: { color: colors.faint, fontSize: 14, letterSpacing: 0 },
  resultRow: { paddingHorizontal: 20, paddingVertical: 15, backgroundColor: colors.surface, borderBottomColor: colors.line, borderBottomWidth: StyleSheet.hairlineWidth },
  resultTitle: { color: colors.ink, fontSize: 15, fontWeight: '700', letterSpacing: 0 },
  resultSnippet: { color: colors.muted, fontSize: 13, lineHeight: 19, marginTop: 4, letterSpacing: 0 },
});
