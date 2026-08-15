import { ImagePlus, Video, X } from 'lucide-react-native';
import { useEffect, useState } from 'react';
import {
  ActivityIndicator,
  KeyboardAvoidingView,
  Modal,
  Platform,
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  TextInput,
  View,
} from 'react-native';
import * as ImagePicker from 'expo-image-picker';

import { getMediaAsset, uploadVideoAsset, type MediaResource } from '../api/client';
import { colors } from '../theme';
import { ContentType, CreatePostInput, GrowthDomain } from '../types';

type Props = {
  visible: boolean;
  onClose: () => void;
  onSubmit: (post: CreatePostInput) => Promise<void>;
};

export function CreatePostModal({ visible, onClose, onSubmit }: Props) {
  const [title, setTitle] = useState('');
  const [summary, setSummary] = useState('');
  const [body, setBody] = useState('');
  const [contentType, setContentType] = useState<ContentType>('note');
  const [domain, setDomain] = useState<GrowthDomain>('learning');
  const [tags, setTags] = useState('');
  const [topics, setTopics] = useState('');
  const [videoResource, setVideoResource] = useState<MediaResource>();
  const [videoName, setVideoName] = useState('');
  const [videoError, setVideoError] = useState<string>();
  const [uploadingVideo, setUploadingVideo] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [submitError, setSubmitError] = useState(false);

  useEffect(() => {
    if (!visible) return;
    setTitle('');
    setSummary('');
    setBody('');
    setContentType('note');
    setDomain('learning');
    setTags('');
    setTopics('');
    setVideoResource(undefined);
    setVideoName('');
    setVideoError(undefined);
    setUploadingVideo(false);
    setSubmitting(false);
    setSubmitError(false);
  }, [visible]);

  useEffect(() => {
    if (!visible || !videoResource || videoResource.status !== 'processing') return;
    let cancelled = false;
    let retryTimer: ReturnType<typeof setTimeout> | undefined;
    const refresh = async () => {
      try {
        const next = await getMediaAsset(videoResource.id);
        if (cancelled) return;
        setVideoResource(next);
        if (next.status === 'processing') retryTimer = setTimeout(() => void refresh(), 2_000);
      } catch {
        if (!cancelled) retryTimer = setTimeout(() => void refresh(), 4_000);
      }
    };
    retryTimer = setTimeout(() => void refresh(), 1_000);
    return () => {
      cancelled = true;
      if (retryTimer) clearTimeout(retryTimer);
    };
  }, [videoResource?.id, videoResource?.status, visible]);

  const pickVideo = async () => {
    setVideoError(undefined);
    const permission = await ImagePicker.requestMediaLibraryPermissionsAsync();
    if (!permission.granted) {
      setVideoError('需要媒体访问权限才能添加视频');
      return;
    }
    const result = await ImagePicker.launchImageLibraryAsync({
      mediaTypes: ['videos'],
      allowsEditing: false,
    });
    if (result.canceled) return;
    const asset = result.assets[0];
    if (!asset?.uri) {
      setVideoError('没有读取到所选视频');
      return;
    }
    setUploadingVideo(true);
    setVideoResource(undefined);
    setVideoName(asset.fileName?.trim() || '已选择视频');
    try {
      const media = await uploadVideoAsset(asset.uri, asset.mimeType);
      setVideoResource(media);
    } catch (error) {
      setVideoName('');
      setVideoError(error instanceof Error ? error.message : '视频上传失败，请重试');
    } finally {
      setUploadingVideo(false);
    }
  };

  const needsReadyVideo = contentType === 'video';
  const videoReady = videoResource?.status === 'ready';
  const ready = title.trim().length > 0 && body.trim().length > 0 && !uploadingVideo && (!needsReadyVideo || videoReady);
  const submit = async () => {
    if (!ready || submitting) return;
    setSubmitting(true);
    setSubmitError(false);
    try {
      await onSubmit({
        title: title.trim(),
        summary: summary.trim() || body.trim().slice(0, 120),
        body: body.trim(),
        domain,
        content_type: contentType,
        media_asset_ids: needsReadyVideo && videoResource ? [videoResource.id] : undefined,
        tags: normalizeWords(tags),
        topics: normalizeWords(topics),
      });
    } catch {
      setSubmitError(true);
      setSubmitting(false);
    }
  };

  return (
    <Modal animationType="slide" onRequestClose={onClose} transparent visible={visible}>
      <KeyboardAvoidingView behavior={Platform.OS === 'ios' ? 'padding' : undefined} style={styles.overlay}>
        <View style={styles.sheet}>
          <View style={styles.header}><View><Text style={styles.title}>发布一条心得</Text><Text style={styles.headerHint}>把真实方法和过程交给社区</Text></View><Pressable accessibilityLabel="关闭内容创作" hitSlop={10} onPress={onClose} style={styles.close}><X color={colors.ink} size={22} /></Pressable></View>
          <ScrollView contentContainerStyle={styles.form} keyboardShouldPersistTaps="handled" showsVerticalScrollIndicator={false}>
            <View style={styles.contentTypePicker}>{contentTypes.map((option) => <Pressable accessibilityRole="radio" accessibilityState={{ checked: contentType === option.type }} key={option.type} onPress={() => setContentType(option.type)} style={({ pressed }) => [styles.contentType, contentType === option.type && styles.contentTypeSelected, pressed && styles.pressed]}><Text style={[styles.contentTypeText, contentType === option.type && styles.contentTypeTextSelected]}>{option.label}</Text></Pressable>)}</View>
            <TextInput accessibilityLabel="内容标题" maxLength={120} onChangeText={setTitle} placeholder="用一句话说清你想分享什么" placeholderTextColor={colors.faint} style={styles.titleInput} value={title} />
            <TextInput accessibilityLabel="内容摘要" maxLength={500} multiline onChangeText={setSummary} placeholder="一句让人愿意继续看的摘要（可选）" placeholderTextColor={colors.faint} style={styles.summaryInput} textAlignVertical="top" value={summary} />
            <TextInput accessibilityLabel="内容正文" maxLength={10000} multiline onChangeText={setBody} placeholder={contentType === 'video' ? '这段视频背后的过程、方法或关键体会是什么？' : '写下过程、方法、踩过的坑，或值得被别人实践的发现。'} placeholderTextColor={colors.faint} style={styles.bodyInput} textAlignVertical="top" value={body} />
            <View style={styles.field}><Text style={styles.label}>所在领域</Text><View style={styles.choiceRow}>{domains.map((option) => <Pressable accessibilityRole="radio" accessibilityState={{ checked: domain === option.domain }} key={option.domain} onPress={() => setDomain(option.domain)} style={({ pressed }) => [styles.choice, domain === option.domain && styles.choiceSelected, pressed && styles.pressed]}><Text style={[styles.choiceText, domain === option.domain && styles.choiceTextSelected]}>{option.label}</Text></Pressable>)}</View></View>
            <View style={styles.field}><Text style={styles.label}>标签和话题</Text><TextInput accessibilityLabel="内容标签" onChangeText={setTags} placeholder="标签（选填，用逗号分隔）" placeholderTextColor={colors.faint} style={styles.input} value={tags} /><TextInput accessibilityLabel="内容话题" onChangeText={setTopics} placeholder="话题（选填，用逗号分隔）" placeholderTextColor={colors.faint} style={styles.input} value={topics} /></View>
            {needsReadyVideo ? <View style={styles.field}><Text style={styles.label}>视频素材</Text><Pressable accessibilityLabel="选择 MP4 视频" disabled={uploadingVideo} onPress={() => void pickVideo()} style={({ pressed }) => [styles.videoPicker, uploadingVideo && styles.disabled, pressed && !uploadingVideo && styles.pressed]}>{uploadingVideo ? <ActivityIndicator color={colors.evergreen} size="small" /> : <Video color={colors.evergreen} size={21} />}<View style={styles.videoPickerCopy}><Text style={styles.videoPickerTitle}>{uploadingVideo ? '正在上传视频' : videoResource ? '更换 MP4 视频' : '选择 MP4 视频'}</Text><Text style={styles.videoPickerText}>{videoStatusLabel(videoResource, videoName)}</Text></View><ImagePlus color={colors.faint} size={18} /></Pressable>{videoError ? <Text accessibilityLiveRegion="polite" style={styles.error}>{videoError}</Text> : null}{videoResource?.status === 'blocked' || videoResource?.status === 'deleted' ? <Text accessibilityLiveRegion="polite" style={styles.error}>该视频未通过安全处理，请重新选择内容。</Text> : null}</View> : null}
            <View style={styles.reviewNotice}><Text style={styles.reviewNoticeTitle}>发布前会做什么</Text><Text style={styles.reviewNoticeText}>内容会先进入审核；视频必须完成 Media 安全处理后才能提交。未经处理的文件不会出现在社区。</Text></View>
            {submitError ? <Text accessibilityLiveRegion="polite" style={styles.error}>提交失败，内容仍保留在这里，可直接重试。</Text> : null}
          </ScrollView>
          <Pressable disabled={!ready || submitting} onPress={() => void submit()} style={({ pressed }) => [styles.submit, (!ready || submitting) && styles.disabled, pressed && ready && !submitting && styles.pressed]}><Text style={styles.submitText}>{submitting ? '正在提交审核…' : !needsReadyVideo ? '提交审核' : uploadingVideo ? '视频上传中' : videoResource?.status === 'processing' ? '视频安全处理中' : '提交审核'}</Text></Pressable>
        </View>
      </KeyboardAvoidingView>
    </Modal>
  );
}

const contentTypes: Array<{ type: Extract<ContentType, 'note' | 'article' | 'video'>; label: string }> = [
  { type: 'note', label: '心得' },
  { type: 'article', label: '长文' },
  { type: 'video', label: '视频' },
];

const domains: Array<{ domain: GrowthDomain; label: string }> = [
  { domain: 'learning', label: '学习' },
  { domain: 'movement', label: '运动' },
  { domain: 'wellness', label: '身心' },
  { domain: 'travel', label: '旅行' },
  { domain: 'leisure', label: '生活' },
];

function normalizeWords(value: string) {
  const seen = new Set<string>();
  return value.split(/[，,\n]/)
    .map((item) => item.trim().replace(/^#/, ''))
    .filter((item) => item && !seen.has(item.toLocaleLowerCase()) && Boolean(seen.add(item.toLocaleLowerCase())))
    .slice(0, 12);
}

function videoStatusLabel(media: MediaResource | undefined, videoName: string) {
  if (!media) return videoName || '仅支持 MP4；上传后会先完成安全处理。';
  if (media.status === 'ready') return '视频已完成安全处理，可以提交审核。';
  if (media.status === 'blocked' || media.status === 'deleted') return '视频未通过处理，不能公开发布。';
  return '视频正在安全处理，完成后会自动解锁提交。';
}

const styles = StyleSheet.create({
  overlay: { flex: 1, justifyContent: 'flex-end', backgroundColor: 'rgba(24, 29, 26, 0.36)' },
  sheet: { maxHeight: '94%', paddingBottom: 24, borderTopLeftRadius: 8, borderTopRightRadius: 8, backgroundColor: colors.surface },
  header: { minHeight: 64, paddingHorizontal: 20, paddingVertical: 11, flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', gap: 12, borderBottomWidth: StyleSheet.hairlineWidth, borderBottomColor: colors.line },
  title: { color: colors.ink, fontSize: 19, fontWeight: '700', letterSpacing: 0 },
  headerHint: { color: colors.muted, fontSize: 11, marginTop: 3, letterSpacing: 0 },
  close: { width: 40, height: 40, alignItems: 'center', justifyContent: 'center' },
  form: { padding: 20, gap: 15 },
  contentTypePicker: { flexDirection: 'row', gap: 8 },
  contentType: { flex: 1, height: 38, alignItems: 'center', justifyContent: 'center', borderWidth: 1, borderColor: colors.line, borderRadius: 5, backgroundColor: colors.background },
  contentTypeSelected: { borderColor: colors.evergreen, backgroundColor: colors.evergreenSoft },
  contentTypeText: { color: colors.muted, fontSize: 12, fontWeight: '700', letterSpacing: 0 },
  contentTypeTextSelected: { color: colors.evergreen },
  titleInput: { height: 48, paddingHorizontal: 12, borderRadius: 6, borderWidth: 1, borderColor: colors.line, color: colors.ink, backgroundColor: colors.background, fontSize: 16, fontWeight: '700', letterSpacing: 0 },
  summaryInput: { minHeight: 68, padding: 12, borderRadius: 6, borderWidth: 1, borderColor: colors.line, color: colors.ink, backgroundColor: colors.background, fontSize: 13, lineHeight: 20, letterSpacing: 0 },
  bodyInput: { minHeight: 152, padding: 12, borderRadius: 6, borderWidth: 1, borderColor: colors.line, color: colors.ink, backgroundColor: colors.background, fontSize: 15, lineHeight: 23, letterSpacing: 0 },
  field: { gap: 8 },
  label: { color: colors.ink, fontSize: 13, fontWeight: '700', letterSpacing: 0 },
  choiceRow: { flexDirection: 'row', flexWrap: 'wrap', gap: 7 },
  choice: { minHeight: 31, paddingHorizontal: 10, alignItems: 'center', justifyContent: 'center', borderWidth: 1, borderColor: colors.line, borderRadius: 15, backgroundColor: colors.background },
  choiceSelected: { borderColor: colors.evergreen, backgroundColor: colors.evergreenSoft },
  choiceText: { color: colors.muted, fontSize: 11, fontWeight: '700', letterSpacing: 0 },
  choiceTextSelected: { color: colors.evergreen },
  input: { height: 42, paddingHorizontal: 11, borderRadius: 6, borderWidth: 1, borderColor: colors.line, color: colors.ink, backgroundColor: colors.background, fontSize: 13, letterSpacing: 0 },
  videoPicker: { minHeight: 68, padding: 11, flexDirection: 'row', alignItems: 'center', gap: 10, borderWidth: 1, borderStyle: 'dashed', borderColor: colors.evergreen, borderRadius: 7, backgroundColor: colors.evergreenSoft },
  videoPickerCopy: { flex: 1, minWidth: 0 },
  videoPickerTitle: { color: colors.ink, fontSize: 13, fontWeight: '700', letterSpacing: 0 },
  videoPickerText: { color: colors.muted, fontSize: 11, lineHeight: 17, marginTop: 3, letterSpacing: 0 },
  reviewNotice: { padding: 13, borderRadius: 7, backgroundColor: colors.blueSoft },
  reviewNoticeTitle: { color: colors.ink, fontSize: 12, fontWeight: '700', letterSpacing: 0 },
  reviewNoticeText: { color: colors.muted, fontSize: 11, lineHeight: 18, marginTop: 4, letterSpacing: 0 },
  error: { color: colors.coral, fontSize: 11, lineHeight: 17, letterSpacing: 0 },
  submit: { height: 51, marginHorizontal: 20, borderRadius: 6, alignItems: 'center', justifyContent: 'center', backgroundColor: colors.evergreen },
  submitText: { color: colors.surface, fontSize: 15, fontWeight: '700', letterSpacing: 0 },
  disabled: { opacity: 0.35 },
  pressed: { opacity: 0.65 },
});
