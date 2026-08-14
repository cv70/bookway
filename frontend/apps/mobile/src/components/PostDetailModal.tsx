import { Bookmark, Heart, MessageCircle, Route, Send, UserPlus, X } from 'lucide-react-native';
import { useEffect, useState } from 'react';
import {
  Image,
  Modal,
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  TextInput,
  View,
  type ImageStyle,
} from 'react-native';

import { colors } from '../theme';
import { Comment, CommunityPost } from '../types';
import { DomainBadge } from './DomainBadge';

type Props = {
  post?: CommunityPost;
  visible: boolean;
  liked: boolean;
  bookmarked: boolean;
  joined: boolean;
  following: boolean;
  comments: Comment[];
  onClose: () => void;
  onLike: (postId: string) => void;
  onBookmark: (postId: string) => void;
  onJoin: (post: CommunityPost) => void;
  onFollow: (post: CommunityPost) => void;
  onComment: (postId: string, body: string) => void;
};

export function PostDetailModal({
  post,
  visible,
  liked,
  bookmarked,
  joined,
  following,
  comments,
  onClose,
  onLike,
  onBookmark,
  onJoin,
  onFollow,
  onComment,
}: Props) {
  const [comment, setComment] = useState('');

  useEffect(() => setComment(''), [post?.id, visible]);
  if (!post) return null;
  const submitComment = () => {
    if (!comment.trim()) return;
    onComment(post.id, comment.trim());
    setComment('');
  };

  return (
    <Modal animationType="slide" onRequestClose={onClose} visible={visible}>
      <View style={styles.screen}>
        <View style={styles.header}>
          <Pressable accessibilityLabel="关闭行记详情" hitSlop={10} onPress={onClose} style={styles.close}><X color={colors.ink} size={22} /></Pressable>
          <Text style={styles.headerTitle}>行记详情</Text>
          <View style={styles.close} />
        </View>
        <ScrollView contentContainerStyle={styles.content} keyboardShouldPersistTaps="handled" showsVerticalScrollIndicator={false}>
          <View style={styles.authorRow}>
            <Image source={{ uri: post.author_avatar_url }} style={styles.avatar as ImageStyle} />
            <View style={styles.authorCopy}><Text style={styles.author}>{post.author_name}</Text><Text style={styles.caption}>真实行动记录</Text></View>
            <Pressable onPress={() => onFollow(post)} style={({ pressed }) => [styles.follow, following && styles.following, pressed && styles.pressed]}><UserPlus color={following ? colors.muted : colors.evergreen} size={16} /><Text style={[styles.followText, following && styles.followingText]}>{following ? '已关注' : '关注'}</Text></Pressable>
          </View>
          <View style={styles.domain}><DomainBadge domain={post.domain} /></View>
          <Text style={styles.title}>{post.title}</Text>
          <Text style={styles.summary}>{post.summary}</Text>
          <Image source={{ uri: post.cover_url }} style={styles.cover as ImageStyle} />
          <View style={styles.tags}>{post.tags.map((tag) => <Text key={tag} style={styles.tag}>#{tag}</Text>)}</View>
          <Pressable disabled={joined} onPress={() => onJoin(post)} style={({ pressed }) => [styles.route, joined && styles.routeJoined, pressed && styles.pressed]}>
            <Route color={colors.evergreen} size={20} />
            <View style={styles.routeCopy}><Text numberOfLines={1} style={styles.routeTitle}>{post.route_title}</Text><Text style={styles.routeMeta}>{post.route_duration} · {post.join_count.toLocaleString()} 人加入</Text></View>
            <Text style={styles.join}>{joined ? '已加入' : '加入路线'}</Text>
          </Pressable>
          <View style={styles.interactions}>
            <Pressable accessibilityLabel="喜欢" onPress={() => onLike(post.id)} style={styles.interaction}><Heart color={liked ? colors.coral : colors.muted} fill={liked ? colors.coral : 'transparent'} size={20} /><Text style={styles.interactionText}>{post.like_count.toLocaleString()}</Text></Pressable>
            <Pressable accessibilityLabel="收藏" onPress={() => onBookmark(post.id)} style={styles.interaction}><Bookmark color={bookmarked ? colors.gold : colors.muted} fill={bookmarked ? colors.gold : 'transparent'} size={20} /><Text style={styles.interactionText}>{bookmarked ? '已收藏' : '收藏'}</Text></Pressable>
            <View style={styles.interaction}><MessageCircle color={colors.muted} size={20} /><Text style={styles.interactionText}>{comments.length}</Text></View>
          </View>
          <View style={styles.commentHeader}><Text style={styles.commentTitle}>评论</Text><Text style={styles.commentMeta}>{comments.length} 条</Text></View>
          <View style={styles.commentComposer}>
            <TextInput onChangeText={setComment} placeholder="写下你的回应" placeholderTextColor={colors.faint} style={styles.commentInput} value={comment} />
            <Pressable accessibilityLabel="发布评论" disabled={!comment.trim()} onPress={submitComment} style={[styles.send, !comment.trim() && styles.sendDisabled]}><Send color={colors.surface} size={16} /></Pressable>
          </View>
          <View style={styles.commentList}>
            {comments.length === 0 ? <Text style={styles.empty}>成为第一个留下回应的人</Text> : comments.map((item) => <View key={item.id} style={styles.comment}><View style={styles.commentAvatar}><Text style={styles.commentInitial}>{item.author_name.slice(0, 1)}</Text></View><View style={styles.commentBody}><Text style={styles.commentAuthor}>{item.author_name}</Text><Text style={styles.commentText}>{item.body}</Text><Text style={styles.commentDate}>{formatDate(item.created_at)}</Text></View></View>)}
          </View>
        </ScrollView>
      </View>
    </Modal>
  );
}

function formatDate(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? '刚刚' : `${date.getMonth() + 1} 月 ${date.getDate()} 日`;
}

const styles = StyleSheet.create({
  screen: { flex: 1, backgroundColor: colors.background },
  header: { height: 64, paddingHorizontal: 16, flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', backgroundColor: colors.surface, borderBottomColor: colors.line, borderBottomWidth: StyleSheet.hairlineWidth },
  close: { width: 40, height: 40, alignItems: 'center', justifyContent: 'center' },
  headerTitle: { flex: 1, textAlign: 'center', color: colors.ink, fontSize: 16, fontWeight: '700', letterSpacing: 0 },
  content: { paddingBottom: 36 },
  authorRow: { paddingHorizontal: 20, paddingTop: 18, flexDirection: 'row', alignItems: 'center', gap: 10 },
  avatar: { width: 42, height: 42, borderRadius: 21, backgroundColor: colors.line },
  authorCopy: { flex: 1, minWidth: 0 },
  author: { color: colors.ink, fontSize: 14, fontWeight: '700', letterSpacing: 0 },
  caption: { color: colors.faint, fontSize: 11, marginTop: 2, letterSpacing: 0 },
  follow: { height: 34, paddingHorizontal: 10, borderRadius: 5, flexDirection: 'row', alignItems: 'center', gap: 5, backgroundColor: colors.evergreenSoft },
  following: { backgroundColor: colors.line },
  followText: { color: colors.evergreen, fontSize: 12, fontWeight: '700', letterSpacing: 0 },
  followingText: { color: colors.muted },
  domain: { paddingHorizontal: 20, marginTop: 20 },
  title: { paddingHorizontal: 20, color: colors.ink, fontSize: 25, lineHeight: 34, fontWeight: '700', marginTop: 12, letterSpacing: 0 },
  summary: { paddingHorizontal: 20, color: colors.muted, fontSize: 14, lineHeight: 23, marginTop: 8, letterSpacing: 0 },
  cover: { width: '100%', aspectRatio: 4 / 3, marginTop: 18, backgroundColor: colors.line },
  tags: { paddingHorizontal: 20, flexDirection: 'row', flexWrap: 'wrap', gap: 10, marginTop: 12 },
  tag: { color: colors.blue, fontSize: 12, fontWeight: '600', letterSpacing: 0 },
  route: { minHeight: 68, marginHorizontal: 20, marginTop: 17, padding: 12, borderRadius: 7, flexDirection: 'row', alignItems: 'center', gap: 10, backgroundColor: colors.surface, borderWidth: 1, borderColor: colors.line },
  routeJoined: { backgroundColor: colors.evergreenSoft },
  routeCopy: { flex: 1, minWidth: 0 },
  routeTitle: { color: colors.ink, fontSize: 13, fontWeight: '700', letterSpacing: 0 },
  routeMeta: { color: colors.faint, fontSize: 11, marginTop: 3, letterSpacing: 0 },
  join: { color: colors.evergreen, fontSize: 12, fontWeight: '700', letterSpacing: 0 },
  interactions: { height: 58, paddingHorizontal: 20, flexDirection: 'row', alignItems: 'center', gap: 24 },
  interaction: { flexDirection: 'row', alignItems: 'center', gap: 5 },
  interactionText: { color: colors.muted, fontSize: 12, letterSpacing: 0 },
  commentHeader: { height: 56, paddingHorizontal: 20, flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', borderTopWidth: 8, borderTopColor: colors.line, backgroundColor: colors.surface },
  commentTitle: { color: colors.ink, fontSize: 15, fontWeight: '700', letterSpacing: 0 },
  commentMeta: { color: colors.faint, fontSize: 11, letterSpacing: 0 },
  commentComposer: { minHeight: 52, marginHorizontal: 20, paddingLeft: 11, flexDirection: 'row', alignItems: 'center', borderRadius: 6, borderWidth: 1, borderColor: colors.line, backgroundColor: colors.background },
  commentInput: { flex: 1, minWidth: 0, paddingVertical: 9, color: colors.ink, fontSize: 13, letterSpacing: 0 },
  send: { width: 38, height: 38, marginRight: 6, alignItems: 'center', justifyContent: 'center', borderRadius: 5, backgroundColor: colors.evergreen },
  sendDisabled: { opacity: 0.35 },
  commentList: { paddingHorizontal: 20, paddingTop: 17, gap: 17 },
  empty: { paddingVertical: 20, textAlign: 'center', color: colors.faint, fontSize: 13, letterSpacing: 0 },
  comment: { flexDirection: 'row', gap: 10 },
  commentAvatar: { width: 30, height: 30, borderRadius: 15, alignItems: 'center', justifyContent: 'center', backgroundColor: colors.evergreenSoft },
  commentInitial: { color: colors.evergreen, fontSize: 12, fontWeight: '700', letterSpacing: 0 },
  commentBody: { flex: 1, minWidth: 0 },
  commentAuthor: { color: colors.ink, fontSize: 12, fontWeight: '700', letterSpacing: 0 },
  commentText: { color: colors.muted, fontSize: 13, lineHeight: 20, marginTop: 3, letterSpacing: 0 },
  commentDate: { color: colors.faint, fontSize: 10, marginTop: 4, letterSpacing: 0 },
  pressed: { opacity: 0.62 },
});
