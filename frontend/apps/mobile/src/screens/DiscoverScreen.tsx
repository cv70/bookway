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

import { search as searchApi } from '../api/client';
import { eventReporter } from '../analytics/eventReporter';
import { FeedCard } from '../components/FeedCard';
import { ScreenHeader } from '../components/ScreenHeader';
import { colors, domainMeta } from '../theme';
import { Feed, GrowthDomain, SearchResult } from '../types';

type Filter = 'all' | GrowthDomain;

const filters: Array<{ key: Filter; label: string }> = [
  { key: 'all', label: '为你推荐' },
  ...Object.entries(domainMeta).map(([key, value]) => ({ key: key as GrowthDomain, label: value.label })),
];

export function DiscoverScreen({
  feed,
  likedPostIds,
  onLike,
}: {
  feed: Feed;
  likedPostIds?: Set<string>;
  onLike?: (postId: string) => void;
}) {
  const [filter, setFilter] = useState<Filter>('all');
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<SearchResult[] | null>(null);
  const [searching, setSearching] = useState(false);
  const items = filter === 'all' ? feed.items : feed.items.filter((item) => item.post.domain === filter);

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
        .then((response) => setResults(response.items))
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

  return (
    <ScrollView contentContainerStyle={styles.content} showsVerticalScrollIndicator={false}>
      <ScreenHeader title="发现" />
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
                  key={result.id}
                  liked={likedPostIds?.has(result.id)}
                  onLike={onLike}
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
                key={item.post.id}
                liked={likedPostIds?.has(item.post.id)}
                onLike={onLike}
              />
            ))}
      </View>
      {((query && results?.length === 0) || (!query && items.length === 0)) ? (
        <View style={styles.empty}><Text style={styles.emptyText}>这一页还在生长</Text></View>
      ) : null}
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  content: { paddingBottom: 24 },
  searchBox: { height: 44, marginHorizontal: 16, marginBottom: 14, paddingHorizontal: 12, flexDirection: 'row', alignItems: 'center', gap: 8, borderWidth: 1, borderColor: colors.line, borderRadius: 7, backgroundColor: colors.surface },
  searchInput: { flex: 1, minWidth: 0, color: colors.ink, fontSize: 14, paddingVertical: 0, letterSpacing: 0 },
  searchState: { height: 42, paddingHorizontal: 20, flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between' },
  searchTitle: { color: colors.ink, fontSize: 15, fontWeight: '700', letterSpacing: 0 },
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
