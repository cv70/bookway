import { ArrowUpRight, Ban, UserPlus, VolumeX, X } from 'lucide-react-native';
import { ActivityIndicator, Image, Modal, Pressable, ScrollView, StyleSheet, Text, View, type ImageStyle } from 'react-native';

import { colors } from '../theme';
import { ContentDetail, PublicAuthor } from '../types';
import { DomainBadge } from './DomainBadge';

type Props = {
  author?: PublicAuthor;
  contents: ContentDetail[];
  nextCursor?: string | null;
  loading: boolean;
  loadingMore: boolean;
  error?: string;
  following: boolean;
  muted: boolean;
  blocked: boolean;
  onClose: () => void;
  onFollow: (authorId: string) => void;
  onSetRelationship: (authorId: string, edge: 'mute' | 'block', active: boolean) => void;
  onOpenContent: (content: ContentDetail) => void;
  onLoadMore: () => void;
};

export function AuthorProfileModal({ author, contents, nextCursor, loading, loadingMore, error, following, muted, blocked, onClose, onFollow, onSetRelationship, onOpenContent, onLoadMore }: Props) {
  if (!author) return null;
  const visibleContents = contents.filter((content) => content.post);
  const initial = author.name.trim().slice(0, 1) || '行';
  return (
    <Modal animationType="slide" onRequestClose={onClose} visible>
      <View style={styles.screen}>
        <View style={styles.header}>
          <Pressable accessibilityLabel="关闭创作者主页" hitSlop={10} onPress={onClose} style={styles.close}><X color={colors.ink} size={22} /></Pressable>
          <Text style={styles.headerTitle}>创作者主页</Text>
          <View style={styles.close} />
        </View>
        <ScrollView contentContainerStyle={styles.content} onScroll={({ nativeEvent }) => {
          if (nativeEvent.layoutMeasurement.height + nativeEvent.contentOffset.y >= nativeEvent.contentSize.height - 180) onLoadMore();
        }} scrollEventThrottle={200} showsVerticalScrollIndicator={false}>
          <View style={styles.authorCard}>
            {author.avatar_url?.trim() ? <Image source={{ uri: author.avatar_url }} style={styles.avatar as ImageStyle} /> : <View style={styles.avatarFallback}><Text style={styles.avatarInitial}>{initial}</Text></View>}
            <View style={styles.authorCopy}><Text style={styles.authorName}>{author.name}</Text><Text style={styles.authorMeta}>公开的方法、行记与行动经验</Text></View>
            <Pressable accessibilityLabel={following ? '取消关注创作者' : '关注创作者'} onPress={() => onFollow(author.id)} style={({ pressed }) => [styles.follow, following && styles.following, pressed && styles.pressed]}><UserPlus color={following ? colors.muted : colors.evergreen} size={16} /><Text style={[styles.followText, following && styles.followingText]}>{following ? '已关注' : '关注'}</Text></Pressable>
          </View>
          <View style={styles.relationships}>
            <Text style={styles.relationshipHint}>控制这个创作者在你的发现页中出现的方式</Text>
            <View style={styles.relationshipActions}>
              <Pressable accessibilityLabel={muted ? '取消静音创作者' : '静音创作者'} onPress={() => onSetRelationship(author.id, 'mute', !muted)} style={({ pressed }) => [styles.relationshipButton, muted && styles.relationshipActive, pressed && styles.pressed]}><VolumeX color={muted ? colors.evergreen : colors.muted} size={15} /><Text style={[styles.relationshipText, muted && styles.relationshipTextActive]}>{muted ? '已静音' : '减少出现'}</Text></Pressable>
              <Pressable accessibilityLabel={blocked ? '取消屏蔽创作者' : '屏蔽创作者'} onPress={() => onSetRelationship(author.id, 'block', !blocked)} style={({ pressed }) => [styles.relationshipButton, blocked && styles.relationshipBlocked, pressed && styles.pressed]}><Ban color={blocked ? colors.coral : colors.muted} size={15} /><Text style={[styles.relationshipText, blocked && styles.relationshipTextBlocked]}>{blocked ? '已屏蔽' : '屏蔽'}</Text></Pressable>
            </View>
          </View>
          <View style={styles.sectionHeader}><Text style={styles.sectionTitle}>公开内容</Text><Text style={styles.sectionMeta}>{loading ? '正在读取' : `${visibleContents.length} 条已加载`}</Text></View>
          {loading ? <View style={styles.loading}><ActivityIndicator color={colors.evergreen} size="small" /><Text style={styles.loadingText}>正在读取公开内容…</Text></View> : null}
          {error ? <View style={styles.error}><Text style={styles.errorText}>{error}</Text></View> : null}
          {!loading && !error && visibleContents.length === 0 ? <View style={styles.empty}><Text style={styles.emptyTitle}>还没有可看的公开内容</Text><Text style={styles.emptyText}>这位创作者公开新的方法或行记后，会显示在这里。</Text></View> : null}
          {visibleContents.map((content) => {
            const post = content.post;
            if (!post) return null;
            return <Pressable accessibilityLabel={`查看${post.title}`} key={content.id} onPress={() => onOpenContent(content)} style={({ pressed }) => [styles.post, pressed && styles.pressed]}>
              {post.cover_url.trim() ? <Image source={{ uri: post.cover_url }} style={styles.cover as ImageStyle} /> : null}
              <View style={styles.postCopy}>
                <View style={styles.postTop}><DomainBadge domain={post.domain} />{post.is_route ? <Text style={styles.routeLabel}>可采用路线</Text> : null}</View>
                <Text numberOfLines={2} style={styles.postTitle}>{post.title}</Text>
                <Text numberOfLines={3} style={styles.postSummary}>{post.summary}</Text>
                <View style={styles.postFooter}><Text style={styles.postMeta}>{post.like_count.toLocaleString()} 人喜欢</Text><ArrowUpRight color={colors.evergreen} size={16} /></View>
              </View>
            </Pressable>;
          })}
          {nextCursor ? <Pressable accessibilityRole="button" disabled={loadingMore} onPress={onLoadMore} style={[styles.loadMore, loadingMore && styles.loadMoreDisabled]}>{loadingMore ? <ActivityIndicator color={colors.evergreen} size="small" /> : <Text style={styles.loadMoreText}>加载更多公开内容</Text>}</Pressable> : null}
        </ScrollView>
      </View>
    </Modal>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1, backgroundColor: colors.background },
  header: { height: 64, paddingHorizontal: 16, flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', backgroundColor: colors.surface, borderBottomWidth: StyleSheet.hairlineWidth, borderBottomColor: colors.line },
  close: { width: 40, height: 40, alignItems: 'center', justifyContent: 'center' },
  headerTitle: { flex: 1, textAlign: 'center', color: colors.ink, fontSize: 16, fontWeight: '700', letterSpacing: 0 },
  content: { paddingBottom: 32 },
  authorCard: { minHeight: 108, paddingHorizontal: 20, paddingVertical: 20, flexDirection: 'row', alignItems: 'center', gap: 12, backgroundColor: colors.surface, borderBottomWidth: 8, borderBottomColor: colors.line },
  avatar: { width: 54, height: 54, borderRadius: 27, backgroundColor: colors.line },
  avatarFallback: { width: 54, height: 54, alignItems: 'center', justifyContent: 'center', borderRadius: 27, backgroundColor: colors.evergreenSoft },
  avatarInitial: { color: colors.evergreen, fontSize: 20, fontWeight: '700', letterSpacing: 0 },
  authorCopy: { flex: 1, minWidth: 0 },
  authorName: { color: colors.ink, fontSize: 17, fontWeight: '700', letterSpacing: 0 },
  authorMeta: { color: colors.muted, fontSize: 11, lineHeight: 17, marginTop: 4, letterSpacing: 0 },
  follow: { height: 34, paddingHorizontal: 10, flexDirection: 'row', alignItems: 'center', gap: 5, borderRadius: 5, backgroundColor: colors.evergreenSoft },
  following: { backgroundColor: colors.line },
  followText: { color: colors.evergreen, fontSize: 12, fontWeight: '700', letterSpacing: 0 },
  followingText: { color: colors.muted },
  relationships: { paddingHorizontal: 20, paddingVertical: 13, gap: 9, backgroundColor: colors.surface, borderBottomWidth: 1, borderBottomColor: colors.line },
  relationshipHint: { color: colors.faint, fontSize: 11, lineHeight: 16, letterSpacing: 0 },
  relationshipActions: { flexDirection: 'row', gap: 8 },
  relationshipButton: { minHeight: 34, paddingHorizontal: 10, flexDirection: 'row', alignItems: 'center', gap: 5, borderRadius: 5, backgroundColor: colors.line },
  relationshipActive: { backgroundColor: colors.evergreenSoft },
  relationshipBlocked: { backgroundColor: colors.coralSoft },
  relationshipText: { color: colors.muted, fontSize: 12, fontWeight: '700', letterSpacing: 0 },
  relationshipTextActive: { color: colors.evergreen },
  relationshipTextBlocked: { color: colors.coral },
  sectionHeader: { height: 54, paddingHorizontal: 20, flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between' },
  sectionTitle: { color: colors.ink, fontSize: 15, fontWeight: '700', letterSpacing: 0 },
  sectionMeta: { color: colors.faint, fontSize: 11, letterSpacing: 0 },
  loading: { minHeight: 130, alignItems: 'center', justifyContent: 'center', gap: 10 },
  loadingText: { color: colors.muted, fontSize: 12, letterSpacing: 0 },
  error: { marginHorizontal: 20, padding: 13, borderRadius: 7, backgroundColor: colors.goldSoft },
  errorText: { color: colors.muted, fontSize: 12, lineHeight: 18, letterSpacing: 0 },
  empty: { minHeight: 160, paddingHorizontal: 32, alignItems: 'center', justifyContent: 'center' },
  emptyTitle: { color: colors.ink, fontSize: 14, fontWeight: '700', letterSpacing: 0 },
  emptyText: { color: colors.muted, fontSize: 12, lineHeight: 19, marginTop: 6, textAlign: 'center', letterSpacing: 0 },
  post: { marginHorizontal: 16, marginBottom: 10, overflow: 'hidden', borderRadius: 7, backgroundColor: colors.surface, borderWidth: 1, borderColor: colors.line },
  cover: { width: '100%', aspectRatio: 16 / 8, backgroundColor: colors.line },
  postCopy: { padding: 14 },
  postTop: { flexDirection: 'row', alignItems: 'center', gap: 8 },
  routeLabel: { color: colors.evergreen, fontSize: 10, fontWeight: '700', letterSpacing: 0 },
  postTitle: { color: colors.ink, fontSize: 16, lineHeight: 23, fontWeight: '700', marginTop: 9, letterSpacing: 0 },
  postSummary: { color: colors.muted, fontSize: 13, lineHeight: 20, marginTop: 5, letterSpacing: 0 },
  postFooter: { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', marginTop: 11 },
  postMeta: { color: colors.faint, fontSize: 11, letterSpacing: 0 },
  loadMore: { minHeight: 42, marginHorizontal: 16, marginTop: 6, alignItems: 'center', justifyContent: 'center', borderRadius: 6, borderWidth: 1, borderColor: colors.line, backgroundColor: colors.surface },
  loadMoreDisabled: { opacity: 0.62 },
  loadMoreText: { color: colors.evergreen, fontSize: 12, fontWeight: '700', letterSpacing: 0 },
  pressed: { opacity: 0.62 },
});
