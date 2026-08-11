import { Bookmark, Heart, Route, UsersRound } from 'lucide-react-native';
import { useEffect } from 'react';
import { Image, Pressable, StyleSheet, Text, View, type ImageStyle } from 'react-native';

import { colors } from '../theme';
import { FeedItem } from '../types';
import { eventReporter } from '../analytics/eventReporter';
import { DomainBadge } from './DomainBadge';

export function FeedCard({
  item,
  liked = false,
  onLike,
}: {
  item: FeedItem;
  liked?: boolean;
  onLike?: (postId: string) => void;
}) {
  const { post } = item;
  useEffect(() => {
    eventReporter.impression(post.id, 'feed-card');
  }, [post.id]);
  return (
    <View style={styles.card}>
      <View style={styles.authorRow}>
        <Image source={{ uri: post.author_avatar_url }} style={styles.avatar as ImageStyle} />
        <View style={styles.authorCopy}>
          <Text style={styles.author}>{post.author_name}</Text>
          <Text numberOfLines={1} style={styles.reason}>{item.reasons[0] ?? '为你推荐'}</Text>
        </View>
        <DomainBadge domain={post.domain} />
      </View>
      <Image source={{ uri: post.cover_url }} style={styles.cover as ImageStyle} />
      <View style={styles.body}>
        <Text style={styles.title}>{post.title}</Text>
        <Text numberOfLines={3} style={styles.summary}>{post.summary}</Text>
        <View style={styles.tags}>
          {post.tags.map((tag) => <Text key={tag} style={styles.tag}>#{tag}</Text>)}
        </View>
        <Pressable style={({ pressed }) => [styles.route, pressed && styles.pressed]}>
          <View style={styles.routeIcon}><Route color={colors.evergreen} size={18} /></View>
          <View style={styles.routeCopy}>
            <Text numberOfLines={1} style={styles.routeTitle}>{post.route_title}</Text>
            <View style={styles.routeMeta}>
              <Text style={styles.routeMetaText}>{post.route_duration}</Text>
              <UsersRound color={colors.faint} size={13} />
              <Text style={styles.routeMetaText}>{post.join_count.toLocaleString()} 人加入</Text>
            </View>
          </View>
          <Text style={styles.join}>加入</Text>
        </Pressable>
        <View style={styles.actions}>
          <Pressable
            accessibilityLabel="喜欢"
            hitSlop={8}
            onPress={() => onLike?.(post.id)}
            style={styles.action}
          >
            <Heart color={liked ? colors.coral : colors.muted} fill={liked ? colors.coral : 'transparent'} size={20} />
            <Text style={styles.actionText}>{compact(post.like_count)}</Text>
          </Pressable>
          <Pressable accessibilityLabel="收藏" hitSlop={8} style={styles.action}>
            <Bookmark color={colors.muted} size={20} />
          </Pressable>
        </View>
      </View>
    </View>
  );
}

function compact(value: number) {
  return value >= 10000 ? `${(value / 10000).toFixed(1)}万` : value.toLocaleString();
}

const styles = StyleSheet.create({
  card: { backgroundColor: colors.surface, borderBottomColor: colors.line, borderBottomWidth: 8 },
  authorRow: { height: 64, paddingHorizontal: 16, flexDirection: 'row', alignItems: 'center', gap: 10 },
  avatar: { width: 36, height: 36, borderRadius: 18, backgroundColor: colors.line },
  authorCopy: { flex: 1, minWidth: 0 },
  author: { color: colors.ink, fontSize: 13, fontWeight: '700', letterSpacing: 0 },
  reason: { color: colors.faint, fontSize: 10, lineHeight: 15, marginTop: 1, letterSpacing: 0 },
  cover: { width: '100%', aspectRatio: 4 / 3, backgroundColor: colors.line },
  body: { paddingHorizontal: 16, paddingTop: 15, paddingBottom: 12 },
  title: { color: colors.ink, fontSize: 20, lineHeight: 28, fontWeight: '700', letterSpacing: 0 },
  summary: { color: colors.muted, fontSize: 14, lineHeight: 22, marginTop: 7, letterSpacing: 0 },
  tags: { flexDirection: 'row', gap: 10, marginTop: 10 },
  tag: { color: colors.blue, fontSize: 12, fontWeight: '600', letterSpacing: 0 },
  route: { flexDirection: 'row', alignItems: 'center', gap: 10, backgroundColor: colors.background, borderRadius: 6, marginTop: 14, padding: 11 },
  pressed: { opacity: 0.62 },
  routeIcon: { width: 32, height: 32, borderRadius: 6, backgroundColor: colors.evergreenSoft, alignItems: 'center', justifyContent: 'center' },
  routeCopy: { flex: 1, minWidth: 0 },
  routeTitle: { color: colors.ink, fontSize: 13, fontWeight: '700', letterSpacing: 0 },
  routeMeta: { flexDirection: 'row', alignItems: 'center', gap: 5, marginTop: 3 },
  routeMetaText: { color: colors.faint, fontSize: 10, letterSpacing: 0 },
  join: { color: colors.evergreen, fontSize: 12, fontWeight: '700', letterSpacing: 0 },
  actions: { height: 34, flexDirection: 'row', alignItems: 'center', gap: 22, marginTop: 10 },
  action: { minWidth: 28, height: 34, flexDirection: 'row', alignItems: 'center', gap: 5 },
  actionText: { color: colors.muted, fontSize: 12, letterSpacing: 0 },
});
