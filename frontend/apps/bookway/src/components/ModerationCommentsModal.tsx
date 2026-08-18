import { Check, RefreshCw, ShieldCheck, ShieldX, X } from 'lucide-react-native';
import { useEffect, useRef, useState } from 'react';
import { ActivityIndicator, Modal, Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';

import { getModerationComments, reviewModerationComment } from '../api/client';
import { colors } from '../theme';
import { Comment } from '../types';

type Props = {
  visible: boolean;
  onClose: () => void;
};

export function ModerationCommentsModal({ visible, onClose }: Props) {
  const [comments, setComments] = useState<Comment[]>([]);
  const [nextCursor, setNextCursor] = useState<string>();
  const [loading, setLoading] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [reviewingId, setReviewingId] = useState<string>();
  const [restrictingId, setRestrictingId] = useState<string>();
  const [error, setError] = useState<string>();
  const [reload, setReload] = useState(0);
  const loadVersionRef = useRef(0);

  useEffect(() => {
    if (!visible) return undefined;
    const version = loadVersionRef.current + 1;
    loadVersionRef.current = version;
    let active = true;
    setLoading(true);
    setError(undefined);
    setRestrictingId(undefined);
    void getModerationComments()
      .then((page) => {
        if (!active || loadVersionRef.current !== version) return;
        setComments(page.items);
        setNextCursor(page.next_cursor ?? undefined);
      })
      .catch(() => {
        if (active && loadVersionRef.current === version) {
          setError('暂时无法读取待审评论，请稍后重试。');
        }
      })
      .finally(() => {
        if (active && loadVersionRef.current === version) setLoading(false);
      });
    return () => {
      active = false;
      if (loadVersionRef.current === version) loadVersionRef.current += 1;
    };
  }, [reload, visible]);

  const refresh = () => {
    if (loading) return;
    setReload((value) => value + 1);
  };

  const loadMore = () => {
    const cursor = nextCursor;
    if (!cursor || loadingMore || loading || reviewingId) return;
    setLoadingMore(true);
    setError(undefined);
    void getModerationComments(cursor)
      .then((page) => {
        setComments((current) => mergeComments(current, page.items));
        setNextCursor(page.next_cursor ?? undefined);
      })
      .catch(() => setError('下一页评论暂时无法读取，请稍后重试。'))
      .finally(() => setLoadingMore(false));
  };

  const review = async (commentId: string, decision: 'approve' | 'restrict') => {
    if (reviewingId) return;
    setReviewingId(commentId);
    setError(undefined);
    try {
      await reviewModerationComment(commentId, decision);
      setComments((current) => current.filter((comment) => comment.id !== commentId));
      setRestrictingId(undefined);
    } catch {
      setError('审核决定未提交。评论可能已被其他审核员处理，请刷新队列确认。');
    } finally {
      setReviewingId(undefined);
    }
  };

  return (
    <Modal animationType="slide" onRequestClose={onClose} presentationStyle="pageSheet" visible={visible}>
      <View style={styles.screen}>
        <View style={styles.header}>
          <Pressable accessibilityLabel="关闭评论审核" hitSlop={10} onPress={onClose} style={styles.close}><X color={colors.ink} size={22} /></Pressable>
          <View style={styles.heading}>
            <Text style={styles.title}>评论审核</Text>
            <Text style={styles.subtitle}>{loading ? '正在读取队列' : `${comments.length} 条待审评论`}</Text>
          </View>
          <Pressable accessibilityLabel="刷新待审评论" disabled={loading || Boolean(reviewingId)} hitSlop={10} onPress={refresh} style={styles.close}>
            {loading ? <ActivityIndicator color={colors.evergreen} size="small" /> : <RefreshCw color={colors.evergreen} size={19} />}
          </Pressable>
        </View>
        <ScrollView
          contentContainerStyle={styles.content}
          onScroll={({ nativeEvent }) => {
            if (nativeEvent.layoutMeasurement.height + nativeEvent.contentOffset.y >= nativeEvent.contentSize.height - 180) loadMore();
          }}
          scrollEventThrottle={200}
          showsVerticalScrollIndicator={false}
        >
          <View style={styles.notice}>
            <ShieldCheck color={colors.evergreen} size={19} />
            <Text style={styles.noticeText}>通过会公开评论；限制会保留审核记录并通知作者。决定提交后不能在此撤销。</Text>
          </View>
          {error ? <View style={styles.error}><Text accessibilityLiveRegion="polite" style={styles.errorText}>{error}</Text><Pressable accessibilityLabel="重试读取评论审核队列" disabled={loading} onPress={refresh} style={styles.retry}><Text style={styles.retryText}>重试</Text></Pressable></View> : null}
          {loading ? <View style={styles.state}><ActivityIndicator color={colors.evergreen} /><Text style={styles.stateText}>正在读取待审评论…</Text></View> : null}
          {!loading && !comments.length ? <View style={styles.empty}><ShieldCheck color={colors.evergreen} size={24} /><Text style={styles.emptyTitle}>审核队列已清空</Text><Text style={styles.emptyText}>新提交且需要人工复核的评论会出现在这里。</Text></View> : null}
          {comments.map((comment) => {
            const reviewing = reviewingId === comment.id;
            const confirmingRestrict = restrictingId === comment.id;
            return <View key={comment.id} style={styles.card}>
              <View style={styles.cardTop}><View style={styles.authorMarker}><Text style={styles.authorInitial}>{comment.author_name.trim().slice(0, 1) || '评'}</Text></View><View style={styles.cardMeta}><Text numberOfLines={1} style={styles.author}>{comment.author_name || '未知作者'}</Text><Text style={styles.meta}>{comment.parent_id ? '回复评论' : '根评论'} · {formatDate(comment.created_at)}</Text></View><Text style={styles.status}>待审</Text></View>
              <Text selectable style={styles.body}>{comment.body}</Text>
              <Text selectable numberOfLines={1} style={styles.postId}>内容 ID：{comment.post_id}</Text>
              {confirmingRestrict ? <View style={styles.confirm}><Text style={styles.confirmText}>确定限制这条评论吗？作者将收到审核结果通知。</Text><View style={styles.actionRow}><Pressable accessibilityLabel="取消限制评论" disabled={reviewing} onPress={() => setRestrictingId(undefined)} style={styles.cancel}><Text style={styles.cancelText}>取消</Text></Pressable><Pressable accessibilityLabel="确认限制评论" disabled={reviewing} onPress={() => void review(comment.id, 'restrict')} style={[styles.restrict, reviewing && styles.actionDisabled]}>{reviewing ? <ActivityIndicator color={colors.surface} size="small" /> : <><ShieldX color={colors.surface} size={15} /><Text style={styles.restrictText}>确认限制</Text></>}</Pressable></View></View> : <View style={styles.actionRow}><Pressable accessibilityLabel="通过并公开评论" disabled={reviewing} onPress={() => void review(comment.id, 'approve')} style={[styles.approve, reviewing && styles.actionDisabled]}>{reviewing ? <ActivityIndicator color={colors.surface} size="small" /> : <><Check color={colors.surface} size={16} /><Text style={styles.approveText}>通过并公开</Text></>}</Pressable><Pressable accessibilityLabel="限制评论" disabled={reviewing} onPress={() => setRestrictingId(comment.id)} style={[styles.limit, reviewing && styles.actionDisabled]}><ShieldX color={colors.coral} size={16} /><Text style={styles.limitText}>限制</Text></Pressable></View>}
            </View>;
          })}
          {nextCursor ? <Pressable accessibilityRole="button" disabled={loadingMore || Boolean(reviewingId)} onPress={loadMore} style={[styles.loadMore, (loadingMore || reviewingId) && styles.loadMoreDisabled]}>{loadingMore ? <ActivityIndicator color={colors.evergreen} size="small" /> : <Text style={styles.loadMoreText}>加载更多待审评论</Text>}</Pressable> : null}
        </ScrollView>
      </View>
    </Modal>
  );
}

function mergeComments(current: Comment[], incoming: Comment[]) {
  const known = new Set(current.map((comment) => comment.id));
  return [...current, ...incoming.filter((comment) => !known.has(comment.id))];
}

function formatDate(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return '时间待确认';
  return new Intl.DateTimeFormat('zh-CN', { month: 'numeric', day: 'numeric', hour: '2-digit', minute: '2-digit' }).format(date);
}

const styles = StyleSheet.create({
  screen: { flex: 1, backgroundColor: colors.background },
  header: { height: 64, paddingHorizontal: 16, flexDirection: 'row', alignItems: 'center', backgroundColor: colors.surface, borderBottomWidth: StyleSheet.hairlineWidth, borderBottomColor: colors.line },
  close: { width: 40, height: 40, alignItems: 'center', justifyContent: 'center' },
  heading: { flex: 1, alignItems: 'center' },
  title: { color: colors.ink, fontSize: 16, fontWeight: '700', letterSpacing: 0 },
  subtitle: { color: colors.faint, fontSize: 10, marginTop: 1, letterSpacing: 0 },
  content: { padding: 16, paddingBottom: 36, gap: 10 },
  notice: { padding: 13, flexDirection: 'row', alignItems: 'flex-start', gap: 10, borderRadius: 7, backgroundColor: colors.evergreenSoft },
  noticeText: { flex: 1, color: colors.muted, fontSize: 12, lineHeight: 18, letterSpacing: 0 },
  error: { padding: 12, flexDirection: 'row', alignItems: 'center', gap: 10, borderRadius: 7, backgroundColor: colors.coralSoft },
  errorText: { flex: 1, color: colors.coral, fontSize: 12, lineHeight: 18, letterSpacing: 0 },
  retry: { minHeight: 30, paddingHorizontal: 9, alignItems: 'center', justifyContent: 'center', borderRadius: 5, backgroundColor: colors.surface },
  retryText: { color: colors.coral, fontSize: 11, fontWeight: '700', letterSpacing: 0 },
  state: { minHeight: 180, alignItems: 'center', justifyContent: 'center', gap: 10 },
  stateText: { color: colors.muted, fontSize: 12, letterSpacing: 0 },
  empty: { minHeight: 220, paddingHorizontal: 30, alignItems: 'center', justifyContent: 'center', gap: 8 },
  emptyTitle: { color: colors.ink, fontSize: 15, fontWeight: '700', letterSpacing: 0 },
  emptyText: { color: colors.muted, fontSize: 12, lineHeight: 19, textAlign: 'center', letterSpacing: 0 },
  card: { padding: 14, borderRadius: 7, borderWidth: 1, borderColor: colors.line, backgroundColor: colors.surface },
  cardTop: { flexDirection: 'row', alignItems: 'center', gap: 9 },
  authorMarker: { width: 32, height: 32, alignItems: 'center', justifyContent: 'center', borderRadius: 6, backgroundColor: colors.goldSoft },
  authorInitial: { color: colors.gold, fontSize: 13, fontWeight: '800', letterSpacing: 0 },
  cardMeta: { flex: 1, minWidth: 0 },
  author: { color: colors.ink, fontSize: 13, fontWeight: '700', letterSpacing: 0 },
  meta: { color: colors.faint, fontSize: 10, marginTop: 2, letterSpacing: 0 },
  status: { color: colors.gold, fontSize: 10, fontWeight: '700', letterSpacing: 0 },
  body: { color: colors.ink, fontSize: 14, lineHeight: 21, marginTop: 12, letterSpacing: 0 },
  postId: { color: colors.faint, fontSize: 10, marginTop: 9, letterSpacing: 0 },
  actionRow: { flexDirection: 'row', gap: 8, marginTop: 14 },
  approve: { flex: 1, minHeight: 38, alignItems: 'center', justifyContent: 'center', flexDirection: 'row', gap: 5, borderRadius: 5, backgroundColor: colors.evergreen },
  approveText: { color: colors.surface, fontSize: 12, fontWeight: '700', letterSpacing: 0 },
  limit: { minHeight: 38, paddingHorizontal: 13, alignItems: 'center', justifyContent: 'center', flexDirection: 'row', gap: 5, borderRadius: 5, borderWidth: 1, borderColor: '#F0C2BC', backgroundColor: colors.coralSoft },
  limitText: { color: colors.coral, fontSize: 12, fontWeight: '700', letterSpacing: 0 },
  actionDisabled: { opacity: 0.62 },
  confirm: { marginTop: 14, padding: 11, borderRadius: 6, backgroundColor: colors.coralSoft },
  confirmText: { color: colors.muted, fontSize: 12, lineHeight: 18, letterSpacing: 0 },
  cancel: { flex: 1, minHeight: 38, alignItems: 'center', justifyContent: 'center', borderRadius: 5, backgroundColor: colors.surface, borderWidth: 1, borderColor: colors.line },
  cancelText: { color: colors.muted, fontSize: 12, fontWeight: '700', letterSpacing: 0 },
  restrict: { flex: 1, minHeight: 38, alignItems: 'center', justifyContent: 'center', flexDirection: 'row', gap: 5, borderRadius: 5, backgroundColor: colors.coral },
  restrictText: { color: colors.surface, fontSize: 12, fontWeight: '700', letterSpacing: 0 },
  loadMore: { minHeight: 42, alignItems: 'center', justifyContent: 'center', borderRadius: 6, borderWidth: 1, borderColor: colors.line, backgroundColor: colors.surface },
  loadMoreDisabled: { opacity: 0.62 },
  loadMoreText: { color: colors.evergreen, fontSize: 12, fontWeight: '700', letterSpacing: 0 },
});
