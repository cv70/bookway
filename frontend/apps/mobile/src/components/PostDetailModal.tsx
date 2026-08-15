import { useEventListener } from 'expo';
import { VideoView, useVideoPlayer } from 'expo-video';
import { Bookmark, EyeOff, Heart, MessageCircle, Route, Send, ShieldAlert, UserPlus, X } from 'lucide-react-native';
import { useEffect, useMemo, useState } from 'react';
import {
  Alert,
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
import { Comment, CommunityPost, ContentDetail, ContentMedia, ReportReason, RouteTemplateAction } from '../types';
import { DomainBadge } from './DomainBadge';

type Props = {
  post?: CommunityPost;
  content?: ContentDetail;
  visible: boolean;
  liked: boolean;
  bookmarked: boolean;
  capturedKnowledge: boolean;
  joined: boolean;
  joining: boolean;
  joinCount?: number;
  following: boolean;
  currentUserId?: string;
  comments: Comment[];
  hasMoreComments: boolean;
  loadingMoreComments: boolean;
  onClose: () => void;
  onLike: (postId: string) => void;
  onBookmark: (postId: string) => void;
  onCaptureKnowledge: (postId: string) => Promise<void>;
  onJoin: (post: CommunityPost) => void;
  onHide: (postId: string) => void;
  onReport: (postId: string, reason: ReportReason) => Promise<void>;
  onFollow: (post: CommunityPost) => void;
  onOpenAuthor: (post: CommunityPost) => void;
  onComment: (postId: string, body: string, parentId?: string) => Promise<Comment>;
  onDeleteComment: (postId: string, commentId: string) => Promise<void>;
  onLoadMoreComments: (postId: string) => Promise<void>;
};

export function PostDetailModal({
  post,
  content,
  visible,
  liked,
  bookmarked,
  capturedKnowledge,
  joined,
  joining,
  joinCount,
  following,
  currentUserId,
  comments,
  hasMoreComments,
  loadingMoreComments,
  onClose,
  onLike,
  onBookmark,
  onCaptureKnowledge,
  onJoin,
  onHide,
  onReport,
  onFollow,
  onOpenAuthor,
  onComment,
  onDeleteComment,
  onLoadMoreComments,
}: Props) {
  const [comment, setComment] = useState('');
  const [replyTo, setReplyTo] = useState<Comment>();
  const [commentSubmitting, setCommentSubmitting] = useState(false);
  const [commentError, setCommentError] = useState(false);
  const [commentNotice, setCommentNotice] = useState('');
  const [loadMoreError, setLoadMoreError] = useState(false);
  const [reporting, setReporting] = useState(false);
  const [reportSubmitting, setReportSubmitting] = useState(false);
  const [reportError, setReportError] = useState(false);
  const [reported, setReported] = useState(false);
  const [deletingCommentId, setDeletingCommentId] = useState<string>();
  const [capturingKnowledge, setCapturingKnowledge] = useState(false);
  const [captureKnowledgeError, setCaptureKnowledgeError] = useState(false);
  const commentsById = useMemo(
    () => new Map(comments.map((item) => [item.id, item])),
    [comments],
  );

  useEffect(() => {
    setComment('');
    setReplyTo(undefined);
    setCommentSubmitting(false);
    setCommentError(false);
    setCommentNotice('');
    setLoadMoreError(false);
    setReporting(false);
    setReportSubmitting(false);
    setReportError(false);
    setReported(false);
    setDeletingCommentId(undefined);
    setCapturingKnowledge(false);
    setCaptureKnowledgeError(false);
  }, [post?.id, visible]);
  if (!post) return null;
  const media = content?.media ?? [];
  const hasMediaImage = media.some((item) => item.kind.toLowerCase() === 'image' && item.url.trim());
  const topics = content?.topics?.filter((topic) => topic.trim()) ?? [];
  const template = content?.route_template;
  const templateStages = template?.stages ?? [];
  const templateActions = template?.actions ?? [];
  const unassignedTemplateActions = templateActions.filter((action) => !Number.isInteger(action.stage_index)
    || action.stage_index === undefined
    || action.stage_index < 0
    || action.stage_index >= templateStages.length);
  const hasRoute = post.is_route ?? Boolean(post.route_title.trim() || template);
  const routeTitle = post.route_title.trim() || post.title;
  const routeDuration = post.route_duration.trim();
  const submitComment = async () => {
    if (!comment.trim() || commentSubmitting) return;
    setCommentSubmitting(true);
    setCommentError(false);
    try {
      const saved = await onComment(post.id, comment.trim(), replyTo?.id);
      setComment('');
      setReplyTo(undefined);
      setCommentNotice(
        saved.status === 'reviewing'
          ? '评论已提交，审核通过后会公开显示'
          : saved.status === 'restricted'
            ? '评论未能公开显示'
            : '',
      );
    } catch {
      setCommentError(true);
    } finally {
      setCommentSubmitting(false);
    }
  };
  const loadMoreComments = async () => {
    setLoadMoreError(false);
    try {
      await onLoadMoreComments(post.id);
    } catch {
      setLoadMoreError(true);
    }
  };
  const submitReport = async (reason: ReportReason) => {
    setReportSubmitting(true);
    setReportError(false);
    try {
      await onReport(post.id, reason);
      setReported(true);
      setReporting(false);
    } catch {
      setReportError(true);
    } finally {
      setReportSubmitting(false);
    }
  };
  const deleteComment = async (item: Comment) => {
    if (deletingCommentId) return;
    setDeletingCommentId(item.id);
    try {
      await onDeleteComment(post.id, item.id);
      if (replyTo?.id === item.id) setReplyTo(undefined);
      setCommentNotice('评论已删除');
    } catch {
      setCommentError(true);
      setCommentNotice('删除失败，请稍后重试');
    } finally {
      setDeletingCommentId(undefined);
    }
  };
  const confirmDeleteComment = (item: Comment) => {
    Alert.alert('删除这条评论？', '删除后正文和作者信息将不再公开。', [
      { text: '取消', style: 'cancel' },
      { text: '删除', style: 'destructive', onPress: () => void deleteComment(item) },
    ]);
  };
  const captureKnowledge = async () => {
    if (capturedKnowledge || capturingKnowledge) return;
    setCapturingKnowledge(true);
    setCaptureKnowledgeError(false);
    try {
      await onCaptureKnowledge(post.id);
    } catch {
      setCaptureKnowledgeError(true);
    } finally {
      setCapturingKnowledge(false);
    }
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
            <Pressable accessibilityLabel={`查看创作者${post.author_name}`} onPress={() => onOpenAuthor(post)} style={styles.authorProfileTarget}>
            <Image source={{ uri: post.author_avatar_url }} style={styles.avatar as ImageStyle} />
            <View style={styles.authorCopy}><Text style={styles.author}>{post.author_name}</Text><Text style={styles.caption}>真实行动记录</Text></View>
            </Pressable>
            <Pressable onPress={() => onFollow(post)} style={({ pressed }) => [styles.follow, following && styles.following, pressed && styles.pressed]}><UserPlus color={following ? colors.muted : colors.evergreen} size={16} /><Text style={[styles.followText, following && styles.followingText]}>{following ? '已关注' : '关注'}</Text></Pressable>
          </View>
          <View style={styles.domain}><DomainBadge domain={post.domain} /></View>
          <Text style={styles.title}>{post.title}</Text>
          <Text style={styles.summary}>{post.summary}</Text>
          {!hasMediaImage && post.cover_url.trim() ? <Image source={{ uri: post.cover_url }} style={styles.cover as ImageStyle} /> : null}
          {content?.body.trim() ? <Text style={styles.detailBody}>{content.body.trim()}</Text> : null}
          {media.length ? <View style={styles.mediaList}>{media.map((item) => {
            const kind = item.kind.toLowerCase();
            if (kind === 'image' && item.url.trim()) {
              return <Image key={item.id} source={{ uri: item.url }} style={[styles.mediaImage as ImageStyle, { aspectRatio: mediaAspectRatio(item.width, item.height) }]} />;
            }
            if (kind === 'video') return <ContentVideo item={item} key={item.id} />;
            return null;
          })}</View> : null}
          <View style={styles.tags}>{post.tags.map((tag) => <Text key={tag} style={styles.tag}>#{tag}</Text>)}</View>
          {topics.length ? <View style={styles.topics}><Text style={styles.topicsLabel}>相关话题</Text>{topics.map((topic) => <Text key={topic} style={styles.topic}>#{topic}</Text>)}</View> : null}
          {template ? <View style={styles.template}>
            <Text style={styles.templateTitle}>路线方法</Text>
            {template.intent.trim() ? <TemplateField label="这条路线想达成什么" value={template.intent} /> : null}
            {template.completion_criteria.trim() ? <TemplateField label="完成标准" value={template.completion_criteria} /> : null}
            {templateStages.length ? <View style={styles.templateSection}><Text style={styles.templateSectionTitle}>阶段安排</Text>{templateStages.map((stage, stageIndex) => {
              const stageActions = templateActions.filter((action) => action.stage_index === stageIndex);
              return <View key={`${stage.title}-${stageIndex}`} style={styles.stageCard}>
                <Text style={styles.stageTitle}>阶段 {stageIndex + 1} · {stage.title || '未命名阶段'}</Text>
                {stage.detail.trim() ? <Text style={styles.stageDetail}>{stage.detail}</Text> : null}
                {stage.completion_criteria.trim() ? <Text style={styles.stageCriteria}>完成：{stage.completion_criteria}</Text> : null}
                {stageActions.map((action, actionIndex) => <RouteTemplateActionCard action={action} key={`${action.title}-${actionIndex}`} />)}
              </View>;
            })}</View> : null}
            {unassignedTemplateActions.length ? <View style={styles.templateSection}><Text style={styles.templateSectionTitle}>{templateStages.length ? '其他行动' : '行动安排'}</Text>{unassignedTemplateActions.map((action, actionIndex) => <RouteTemplateActionCard action={action} key={`${action.title}-${actionIndex}`} />)}</View> : null}
          </View> : null}
          {hasRoute ? <Pressable disabled={joined || joining} onPress={() => onJoin(post)} style={({ pressed }) => [styles.route, joined && styles.routeJoined, pressed && styles.pressed]}>
            <Route color={colors.evergreen} size={20} />
            <View style={styles.routeCopy}><Text numberOfLines={1} style={styles.routeTitle}>{routeTitle}</Text><Text style={styles.routeMeta}>{routeDuration ? `${routeDuration} · ` : ''}{(joinCount ?? post.join_count).toLocaleString()} 人加入</Text></View>
            <Text style={styles.join}>{joining ? '加入中' : joined ? '已加入' : '加入路线'}</Text>
          </Pressable> : null}
          <View style={styles.knowledgeCapture}>
            <View style={styles.knowledgeCaptureCopy}>
              <View style={styles.knowledgeCaptureTitleRow}><Bookmark color={capturedKnowledge ? colors.evergreen : colors.gold} fill={capturedKnowledge ? colors.evergreen : 'transparent'} size={18} /><Text style={styles.knowledgeCaptureTitle}>{capturedKnowledge ? '已收进知识库' : '把这条灵感收进知识库'}</Text></View>
              <Text style={styles.knowledgeCaptureDetail}>{capturedKnowledge ? '在“资源与知识库”的收集箱里，随时把它变成行动计划。' : '只保存标题、摘要和来源；再次打开原内容时仍会校验可见性。'}</Text>
              {captureKnowledgeError ? <Text accessibilityLiveRegion="polite" style={styles.knowledgeCaptureError}>收集失败，请检查网络后重试</Text> : null}
            </View>
            <Pressable accessibilityLabel="收进知识库" disabled={capturedKnowledge || capturingKnowledge} onPress={() => void captureKnowledge()} style={({ pressed }) => [styles.knowledgeCaptureButton, (capturedKnowledge || capturingKnowledge) && styles.knowledgeCaptureButtonDone, pressed && !capturedKnowledge && !capturingKnowledge && styles.pressed]}>
              <Text style={[styles.knowledgeCaptureButtonText, (capturedKnowledge || capturingKnowledge) && styles.knowledgeCaptureButtonTextDone]}>{capturingKnowledge ? '收集中' : capturedKnowledge ? '已收进' : '收进'}</Text>
            </Pressable>
          </View>
          <View style={styles.interactions}>
            <Pressable accessibilityLabel="喜欢" onPress={() => onLike(post.id)} style={styles.interaction}><Heart color={liked ? colors.coral : colors.muted} fill={liked ? colors.coral : 'transparent'} size={20} /><Text style={styles.interactionText}>{post.like_count.toLocaleString()}</Text></Pressable>
            <Pressable accessibilityLabel="收藏" onPress={() => onBookmark(post.id)} style={styles.interaction}><Bookmark color={bookmarked ? colors.gold : colors.muted} fill={bookmarked ? colors.gold : 'transparent'} size={20} /><Text style={styles.interactionText}>{bookmarked ? '已收藏' : '收藏'}</Text></Pressable>
            <View style={styles.interaction}><MessageCircle color={colors.muted} size={20} /><Text style={styles.interactionText}>{comments.length}{hasMoreComments ? '+' : ''}</Text></View>
            <Pressable accessibilityLabel="减少此类内容" onPress={() => onHide(post.id)} style={styles.interaction}><EyeOff color={colors.muted} size={20} /><Text style={styles.interactionText}>不感兴趣</Text></Pressable>
            <Pressable accessibilityLabel="举报内容" disabled={reported || reportSubmitting} onPress={() => setReporting((value) => !value)} style={styles.interaction}><ShieldAlert color={reported ? colors.evergreen : colors.muted} size={20} /><Text style={styles.interactionText}>{reported ? '已提交' : reportSubmitting ? '提交中' : '举报'}</Text></Pressable>
          </View>
          {reporting && !reported ? <View style={styles.reportPanel}><Text style={styles.reportTitle}>请选择举报原因</Text><View style={styles.reportReasons}>{reportReasons.map(({ reason, label }) => <Pressable disabled={reportSubmitting} key={reason} onPress={() => void submitReport(reason)} style={({ pressed }) => [styles.reportReason, pressed && styles.pressed]}><Text style={styles.reportReasonText}>{label}</Text></Pressable>)}</View>{reportError ? <Text accessibilityLiveRegion="polite" style={styles.reportError}>提交失败，请稍后重试</Text> : null}</View> : null}
          <View style={styles.commentHeader}><Text style={styles.commentTitle}>评论</Text><Text style={styles.commentMeta}>{hasMoreComments ? `已加载 ${comments.length} 条` : `${comments.length} 条`}</Text></View>
          {replyTo ? <View style={styles.replyContext}><Text numberOfLines={1} style={styles.replyContextText}>回复 {replyTo.author_name}：{replyTo.body}</Text><Pressable accessibilityLabel="取消回复" hitSlop={8} onPress={() => setReplyTo(undefined)}><X color={colors.muted} size={15} /></Pressable></View> : null}
          <View style={styles.commentComposer}>
            <TextInput maxLength={1000} multiline onChangeText={(value) => { setComment(value); setCommentError(false); setCommentNotice(''); }} placeholder={replyTo ? `回复 ${replyTo.author_name}` : '写下你的回应'} placeholderTextColor={colors.faint} style={styles.commentInput} value={comment} />
            <Pressable accessibilityLabel="发布评论" disabled={!comment.trim() || commentSubmitting} onPress={() => void submitComment()} style={[styles.send, (!comment.trim() || commentSubmitting) && styles.sendDisabled]}><Send color={colors.surface} size={16} /></Pressable>
          </View>
          <View style={styles.composerMeta}><Text accessibilityLiveRegion="polite" style={[styles.commentFeedback, commentError && styles.commentError]}>{commentError ? '发送失败，草稿已保留，请重试' : commentSubmitting ? '正在发送…' : commentNotice || (replyTo ? '你的回复会显示在原评论下方' : '')}</Text>{comment.length ? <Text style={styles.characterCount}>{comment.length}/1000</Text> : null}</View>
          <View style={styles.commentList}>
            {comments.length === 0 ? <Text style={styles.empty}>成为第一个留下回应的人</Text> : comments.map((item) => {
              const parent = item.parent_id ? commentsById.get(item.parent_id) : undefined;
              const isPublic = !item.status || item.status === 'published';
              const isDeleted = item.status === 'deleted';
              const canDelete = Boolean(currentUserId && item.author_id === currentUserId && !isDeleted && !item.id.startsWith('local-comment-'));
              return <View key={item.id} style={[styles.comment, item.parent_id && styles.commentReply]}><View style={styles.commentAvatar}><Text style={styles.commentInitial}>{item.author_name.slice(0, 1)}</Text></View><View style={styles.commentBody}><Text style={styles.commentAuthor}>{item.author_name}{parent ? <Text style={styles.replyAuthor}> 回复 {parent.author_name}</Text> : null}</Text><Text style={styles.commentText}>{item.body}</Text><View style={styles.commentFooter}><Text style={styles.commentDate}>{isDeleted ? '已删除' : item.status === 'reviewing' ? '审核中' : item.status === 'restricted' ? '未公开' : formatDate(item.created_at)}</Text><Pressable accessibilityLabel={`回复 ${item.author_name}`} disabled={item.id.startsWith('local-comment-') || !isPublic || isDeleted} hitSlop={6} onPress={() => { setReplyTo(item); setCommentError(false); setCommentNotice(''); }}><Text style={[styles.replyButton, (!isPublic || isDeleted) && styles.replyButtonDisabled]}>回复</Text></Pressable>{canDelete ? <Pressable accessibilityLabel="删除评论" disabled={deletingCommentId === item.id} hitSlop={6} onPress={() => confirmDeleteComment(item)}><Text style={[styles.deleteCommentButton, deletingCommentId === item.id && styles.replyButtonDisabled]}>{deletingCommentId === item.id ? '删除中' : '删除'}</Text></Pressable> : null}</View></View></View>;
            })}
            {hasMoreComments ? <Pressable disabled={loadingMoreComments} onPress={() => void loadMoreComments()} style={({ pressed }) => [styles.loadMore, pressed && styles.pressed]}><Text style={styles.loadMoreText}>{loadingMoreComments ? '正在加载…' : loadMoreError ? '加载失败，点击重试' : '查看更多回应'}</Text></Pressable> : null}
          </View>
        </ScrollView>
      </View>
    </Modal>
  );
}

const reportReasons: Array<{ reason: ReportReason; label: string }> = [
  { reason: 'spam', label: '垃圾广告' },
  { reason: 'harassment', label: '攻击骚扰' },
  { reason: 'unsafe', label: '危险内容' },
  { reason: 'misinformation', label: '虚假信息' },
  { reason: 'privacy', label: '隐私泄露' },
  { reason: 'other', label: '其他问题' },
];

function TemplateField({ label, value }: { label: string; value: string }) {
  return <View style={styles.templateField}><Text style={styles.templateFieldLabel}>{label}</Text><Text style={styles.templateFieldValue}>{value}</Text></View>;
}

function RouteTemplateActionCard({ action }: { action: RouteTemplateAction }) {
  const duration = action.estimated_minutes > 0 ? `约 ${action.estimated_minutes} 分钟` : '用时自定';
  // Templates describe a reusable method, never the author's calendar.
  const schedule = '按自己的节奏安排';
  return <View style={styles.templateAction}>
    <Text style={styles.templateActionTitle}>{action.title || '行动'}</Text>
    {action.detail.trim() ? <Text style={styles.templateActionDetail}>{action.detail}</Text> : null}
    <Text style={styles.templateActionMeta}>{duration} · {schedule}</Text>
  </View>;
}

function ContentVideo({ item }: { item: ContentMedia }) {
  const url = item.url.trim();
  const [unavailable, setUnavailable] = useState(!url);
  const player = useVideoPlayer(url ? { uri: url, useCaching: true } : null);

  useEffect(() => {
    setUnavailable(!url);
  }, [url]);
  useEventListener(player, 'statusChange', ({ status }) => {
    if (status === 'error') setUnavailable(true);
  });

  const retry = () => {
    if (!url) return;
    setUnavailable(false);
    void player.replaceAsync({ uri: url, useCaching: true }).catch(() => setUnavailable(true));
  };

  if (unavailable) {
    return <View accessibilityLabel="视频暂时无法播放" style={styles.videoUnavailable}><Text style={styles.videoMediaTitle}>视频暂时无法播放</Text><Text style={styles.videoMediaMeta}>{item.duration_ms ? `时长 ${formatDuration(item.duration_ms)}，请稍后重试` : '请稍后重试'}</Text>{url ? <Pressable accessibilityLabel="重试播放视频" onPress={retry} style={({ pressed }) => [styles.videoRetry, pressed && styles.pressed]}><Text style={styles.videoRetryText}>重试播放</Text></Pressable> : null}</View>;
  }

  return <View accessibilityLabel="视频素材" style={styles.videoMedia}>
    <VideoView
      contentFit="contain"
      fullscreenOptions={{ enable: true }}
      nativeControls
      player={player}
      playsInline
      style={[styles.videoPlayer, { aspectRatio: mediaAspectRatio(item.width, item.height) }]}
    />
    {item.duration_ms ? <Text style={styles.videoMediaMeta}>时长 {formatDuration(item.duration_ms)}</Text> : null}
  </View>;
}

function mediaAspectRatio(width: number, height: number) {
  return width > 0 && height > 0 ? width / height : 4 / 3;
}

function formatDuration(durationMs: number) {
  const seconds = Math.max(1, Math.round(durationMs / 1000));
  const minutes = Math.floor(seconds / 60);
  return minutes ? `${minutes} 分钟` : `${seconds} 秒`;
}

function formatDate(value: string) {
  const date = /^\d+$/.test(value) ? new Date(Number(value)) : new Date(value);
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
  authorProfileTarget: { flex: 1, minWidth: 0, flexDirection: 'row', alignItems: 'center', gap: 10 },
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
  detailBody: { paddingHorizontal: 20, color: colors.ink, fontSize: 15, lineHeight: 25, marginTop: 18, letterSpacing: 0 },
  mediaList: { marginTop: 14, gap: 10 },
  mediaImage: { width: '100%', backgroundColor: colors.line },
  videoMedia: { marginHorizontal: 20, overflow: 'hidden', borderRadius: 7, backgroundColor: colors.ink },
  videoPlayer: { width: '100%', backgroundColor: colors.ink },
  videoUnavailable: { minHeight: 96, marginHorizontal: 20, padding: 16, justifyContent: 'center', borderRadius: 7, backgroundColor: colors.ink },
  videoMediaTitle: { color: colors.surface, fontSize: 14, fontWeight: '700', letterSpacing: 0 },
  videoMediaMeta: { paddingHorizontal: 12, paddingVertical: 9, color: colors.line, fontSize: 12, letterSpacing: 0 },
  videoRetry: { alignSelf: 'flex-start', marginHorizontal: 12, marginBottom: 10, paddingHorizontal: 10, paddingVertical: 7, borderRadius: 5, backgroundColor: colors.surface },
  videoRetryText: { color: colors.evergreen, fontSize: 12, fontWeight: '700', letterSpacing: 0 },
  tags: { paddingHorizontal: 20, flexDirection: 'row', flexWrap: 'wrap', gap: 10, marginTop: 12 },
  tag: { color: colors.blue, fontSize: 12, fontWeight: '600', letterSpacing: 0 },
  topics: { marginHorizontal: 20, marginTop: 15, paddingTop: 13, flexDirection: 'row', flexWrap: 'wrap', alignItems: 'center', gap: 9, borderTopWidth: StyleSheet.hairlineWidth, borderTopColor: colors.line },
  topicsLabel: { color: colors.faint, fontSize: 11, fontWeight: '700', letterSpacing: 0 },
  topic: { color: colors.evergreen, fontSize: 12, fontWeight: '600', letterSpacing: 0 },
  template: { marginTop: 18, paddingVertical: 17, backgroundColor: colors.evergreenSoft, borderTopWidth: 1, borderBottomWidth: 1, borderColor: colors.line },
  templateTitle: { paddingHorizontal: 20, color: colors.ink, fontSize: 17, fontWeight: '700', letterSpacing: 0 },
  templateField: { paddingHorizontal: 20, marginTop: 14 },
  templateFieldLabel: { color: colors.evergreen, fontSize: 11, fontWeight: '700', letterSpacing: 0 },
  templateFieldValue: { color: colors.ink, fontSize: 14, lineHeight: 22, marginTop: 4, letterSpacing: 0 },
  templateSection: { marginTop: 18, paddingHorizontal: 20, gap: 10 },
  templateSectionTitle: { color: colors.ink, fontSize: 13, fontWeight: '700', letterSpacing: 0 },
  stageCard: { padding: 13, borderRadius: 7, gap: 6, backgroundColor: colors.surface, borderWidth: 1, borderColor: colors.line },
  stageTitle: { color: colors.ink, fontSize: 13, fontWeight: '700', letterSpacing: 0 },
  stageDetail: { color: colors.muted, fontSize: 13, lineHeight: 20, letterSpacing: 0 },
  stageCriteria: { color: colors.evergreen, fontSize: 12, lineHeight: 19, letterSpacing: 0 },
  templateAction: { marginTop: 5, padding: 11, borderRadius: 5, backgroundColor: colors.background },
  templateActionTitle: { color: colors.ink, fontSize: 13, fontWeight: '700', letterSpacing: 0 },
  templateActionDetail: { color: colors.muted, fontSize: 12, lineHeight: 19, marginTop: 4, letterSpacing: 0 },
  templateActionMeta: { color: colors.faint, fontSize: 11, marginTop: 5, letterSpacing: 0 },
  route: { minHeight: 68, marginHorizontal: 20, marginTop: 17, padding: 12, borderRadius: 7, flexDirection: 'row', alignItems: 'center', gap: 10, backgroundColor: colors.surface, borderWidth: 1, borderColor: colors.line },
  routeJoined: { backgroundColor: colors.evergreenSoft },
  routeCopy: { flex: 1, minWidth: 0 },
  routeTitle: { color: colors.ink, fontSize: 13, fontWeight: '700', letterSpacing: 0 },
  routeMeta: { color: colors.faint, fontSize: 11, marginTop: 3, letterSpacing: 0 },
  join: { color: colors.evergreen, fontSize: 12, fontWeight: '700', letterSpacing: 0 },
  knowledgeCapture: { minHeight: 88, marginHorizontal: 20, marginTop: 15, padding: 13, flexDirection: 'row', alignItems: 'center', gap: 12, borderRadius: 7, backgroundColor: colors.surface, borderWidth: 1, borderColor: colors.line },
  knowledgeCaptureCopy: { flex: 1, minWidth: 0 },
  knowledgeCaptureTitleRow: { flexDirection: 'row', alignItems: 'center', gap: 7 },
  knowledgeCaptureTitle: { color: colors.ink, fontSize: 13, fontWeight: '700', letterSpacing: 0 },
  knowledgeCaptureDetail: { color: colors.muted, fontSize: 11, lineHeight: 17, marginTop: 6, letterSpacing: 0 },
  knowledgeCaptureError: { color: colors.coral, fontSize: 11, lineHeight: 17, marginTop: 5, letterSpacing: 0 },
  knowledgeCaptureButton: { minWidth: 52, height: 34, paddingHorizontal: 10, alignItems: 'center', justifyContent: 'center', borderRadius: 5, backgroundColor: colors.evergreen },
  knowledgeCaptureButtonDone: { backgroundColor: colors.evergreenSoft },
  knowledgeCaptureButtonText: { color: colors.surface, fontSize: 12, fontWeight: '700', letterSpacing: 0 },
  knowledgeCaptureButtonTextDone: { color: colors.evergreen },
  interactions: { minHeight: 58, paddingHorizontal: 20, paddingVertical: 10, flexDirection: 'row', flexWrap: 'wrap', alignItems: 'center', gap: 18 },
  interaction: { flexDirection: 'row', alignItems: 'center', gap: 5 },
  interactionText: { color: colors.muted, fontSize: 12, letterSpacing: 0 },
  reportPanel: { marginHorizontal: 20, marginBottom: 16, padding: 14, borderRadius: 7, borderWidth: 1, borderColor: colors.line, backgroundColor: colors.surface },
  reportTitle: { color: colors.ink, fontSize: 13, fontWeight: '700', letterSpacing: 0 },
  reportReasons: { flexDirection: 'row', flexWrap: 'wrap', gap: 8, marginTop: 10 },
  reportReason: { minHeight: 34, paddingHorizontal: 11, alignItems: 'center', justifyContent: 'center', borderRadius: 5, backgroundColor: colors.background, borderWidth: 1, borderColor: colors.line },
  reportReasonText: { color: colors.muted, fontSize: 12, fontWeight: '600', letterSpacing: 0 },
  reportError: { color: colors.coral, fontSize: 12, marginTop: 10 },
  commentHeader: { height: 56, paddingHorizontal: 20, flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', borderTopWidth: 8, borderTopColor: colors.line, backgroundColor: colors.surface },
  commentTitle: { color: colors.ink, fontSize: 15, fontWeight: '700', letterSpacing: 0 },
  commentMeta: { color: colors.faint, fontSize: 11, letterSpacing: 0 },
  commentComposer: { minHeight: 52, marginHorizontal: 20, paddingLeft: 11, flexDirection: 'row', alignItems: 'center', borderRadius: 6, borderWidth: 1, borderColor: colors.line, backgroundColor: colors.background },
  replyContext: { minHeight: 34, marginHorizontal: 20, marginBottom: 7, paddingHorizontal: 10, flexDirection: 'row', alignItems: 'center', gap: 8, borderRadius: 5, backgroundColor: colors.evergreenSoft },
  replyContextText: { flex: 1, minWidth: 0, color: colors.evergreen, fontSize: 11, letterSpacing: 0 },
  commentInput: { flex: 1, minWidth: 0, maxHeight: 96, paddingVertical: 9, color: colors.ink, fontSize: 13, letterSpacing: 0 },
  send: { width: 38, height: 38, marginRight: 6, alignItems: 'center', justifyContent: 'center', borderRadius: 5, backgroundColor: colors.evergreen },
  sendDisabled: { opacity: 0.35 },
  composerMeta: { minHeight: 24, paddingHorizontal: 20, paddingTop: 5, flexDirection: 'row', alignItems: 'flex-start', gap: 8 },
  commentFeedback: { flex: 1, minWidth: 0, color: colors.faint, fontSize: 10, lineHeight: 15, letterSpacing: 0 },
  commentError: { color: colors.coral },
  characterCount: { color: colors.faint, fontSize: 10, lineHeight: 15, letterSpacing: 0 },
  commentList: { paddingHorizontal: 20, paddingTop: 10, gap: 17 },
  empty: { paddingVertical: 20, textAlign: 'center', color: colors.faint, fontSize: 13, letterSpacing: 0 },
  comment: { flexDirection: 'row', gap: 10 },
  commentReply: { marginLeft: 30, paddingLeft: 10, borderLeftWidth: 2, borderLeftColor: colors.evergreenSoft },
  commentAvatar: { width: 30, height: 30, borderRadius: 15, alignItems: 'center', justifyContent: 'center', backgroundColor: colors.evergreenSoft },
  commentInitial: { color: colors.evergreen, fontSize: 12, fontWeight: '700', letterSpacing: 0 },
  commentBody: { flex: 1, minWidth: 0 },
  commentAuthor: { color: colors.ink, fontSize: 12, fontWeight: '700', letterSpacing: 0 },
  replyAuthor: { color: colors.faint, fontWeight: '500' },
  commentText: { color: colors.muted, fontSize: 13, lineHeight: 20, marginTop: 3, letterSpacing: 0 },
  commentFooter: { flexDirection: 'row', alignItems: 'center', gap: 14, marginTop: 4 },
  commentDate: { color: colors.faint, fontSize: 10, letterSpacing: 0 },
  replyButton: { color: colors.evergreen, fontSize: 10, fontWeight: '700', letterSpacing: 0 },
  replyButtonDisabled: { color: colors.faint },
  deleteCommentButton: { color: colors.coral, fontSize: 10, fontWeight: '700', letterSpacing: 0 },
  loadMore: { minHeight: 42, alignItems: 'center', justifyContent: 'center', borderTopWidth: StyleSheet.hairlineWidth, borderTopColor: colors.line },
  loadMoreText: { color: colors.evergreen, fontSize: 12, fontWeight: '700', letterSpacing: 0 },
  pressed: { opacity: 0.62 },
});
