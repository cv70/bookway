import { Feed, Journey, ReadingBook, Today } from '../types';

export const fallbackToday: Today = {
  completed: 1,
  total: 3,
  focus_minutes: 8,
  actions: [
    {
      id: 'action-read-city',
      journey_id: 'journey-reading',
      title: '阅读第三章',
      detail: '标记一个关于城市空间的观点',
      estimated_minutes: 30,
      scheduled_label: '上午',
      state: 'pending',
    },
    {
      id: 'action-easy-run',
      journey_id: 'journey-running',
      title: '轻松跑 25 分钟',
      detail: '保持可以自然说话的配速',
      estimated_minutes: 25,
      scheduled_label: '傍晚',
      state: 'pending',
    },
    {
      id: 'action-stretch',
      journey_id: 'journey-running',
      title: '跑后拉伸',
      detail: '完成小腿与髋部拉伸',
      estimated_minutes: 8,
      scheduled_label: '傍晚',
      state: 'completed',
    },
  ],
};

export const fallbackJourneys: Journey[] = [
  {
    id: 'journey-reading',
    title: '读懂现代城市',
    intent: '用阅读建立观察一座城市的方法',
    domain: 'learning',
    status: 'active',
    progress: 36,
    duration_label: '6 周',
    next_action: '阅读《看不见的城市》第三章',
    participant_count: 1284,
  },
  {
    id: 'journey-running',
    title: '重新跑起来',
    intent: '以不受伤的方式恢复规律运动',
    domain: 'movement',
    status: 'active',
    progress: 58,
    duration_label: '4 周',
    next_action: '轻松跑 25 分钟',
    participant_count: 3276,
  },
];

export const fallbackFeed: Feed = {
  request_id: 'local-preview',
  meta: { sourced: 5, filtered: 0, selected: 5 },
  items: [
    {
      score: 4.82,
      source: 'recommend-main:quality',
      reasons: ['符合你的旅行兴趣', '4862 人正在同行'],
      post: {
        id: 'post-city-walk',
        author_name: '木川',
        author_avatar_url:
          'https://images.unsplash.com/photo-1500648767791-00dcc994a43e?w=160&h=160&fit=crop',
        title: '我用 7 次散步重新认识了杭州',
        summary: '不赶景点，只沿着水系和旧城慢慢走。每次回来，我都画一张自己的城市地图。',
        domain: 'travel',
        cover_url:
          'https://images.unsplash.com/photo-1537531383496-f4749b8032cf?w=1200&h=900&fit=crop',
        route_title: '7 次城市观察散步',
        route_duration: '3 周',
        join_count: 4862,
        like_count: 9128,
        freshness: 0.94,
        tags: ['城市漫游', '观察'],
      },
    },
    {
      score: 4.78,
      source: 'recommend-main:quality',
      reasons: ['符合你的学习兴趣', '7130 人正在同行'],
      post: {
        id: 'post-reading',
        author_name: '一册',
        author_avatar_url:
          'https://images.unsplash.com/photo-1494790108377-be9c29b29330?w=160&h=160&fit=crop',
        title: '读完 12 本书后，我留下了这套主题阅读法',
        summary: '从问题出发选择三本结构不同的书，每周只整理一个能用于生活的结论。',
        domain: 'learning',
        cover_url:
          'https://images.unsplash.com/photo-1495446815901-a7297e633e8d?w=1200&h=900&fit=crop',
        route_title: '四周主题阅读实验',
        route_duration: '4 周',
        join_count: 7130,
        like_count: 15420,
        freshness: 0.88,
        tags: ['阅读', '知识管理'],
      },
    },
    {
      score: 4.75,
      source: 'recommend-main:quality',
      reasons: ['符合你的运动兴趣', '9854 人正在同行'],
      post: {
        id: 'post-running',
        author_name: '长风',
        author_avatar_url:
          'https://images.unsplash.com/photo-1534528741775-53994a69daeb?w=160&h=160&fit=crop',
        title: '从跑不动两公里，到享受清晨的五公里',
        summary: '真正有用的不是逼自己更快，而是给身体足够的恢复时间，并记录每次感受。',
        domain: 'movement',
        cover_url:
          'https://images.unsplash.com/photo-1552674605-db6ffd4facb5?w=1200&h=900&fit=crop',
        route_title: '零压力晨跑计划',
        route_duration: '6 周',
        join_count: 9854,
        like_count: 22180,
        freshness: 0.91,
        tags: ['跑步', '晨间'],
      },
    },
    {
      score: 2.43,
      source: 'recommend-main:quality',
      reasons: ['2176 人正在同行'],
      post: {
        id: 'post-pottery',
        author_name: '未名',
        author_avatar_url:
          'https://images.unsplash.com/photo-1531123897727-8f129e1688ce?w=160&h=160&fit=crop',
        title: '周末做陶，让时间重新慢下来',
        summary: '手上的泥总有自己的脾气。两个周末之后，我不再急着控制最后的样子。',
        domain: 'leisure',
        cover_url:
          'https://images.unsplash.com/photo-1610701596007-11502861dcfa?w=1200&h=900&fit=crop',
        route_title: '陶艺初体验',
        route_duration: '2 周',
        join_count: 2176,
        like_count: 6890,
        freshness: 0.96,
        tags: ['手作', '放松'],
      },
    },
  ],
};

export const fallbackReadingBooks: ReadingBook[] = [
  {
    id: 'book-city-observation',
    title: '城市观察练习册',
    author: '万卷行编辑部',
    summary: '用六次短阅读和步行练习，建立观察一座城市的个人方法。',
    journey_id: 'journey-reading',
    progress: 36,
    current_chapter: 2,
    reading_seconds: 1620,
    added_at: '2026-08-01T08:00:00.000Z',
    last_opened_at: '2026-08-13T20:30:00.000Z',
    accent: '#39759B',
    chapters: [
      {
        id: 'city-01',
        title: '从一个问题出发',
        body: [
          '阅读不必从“我应该知道什么”开始。给自己留一个具体的问题，例如：这座城市的人怎样安排一天？问题会替你筛选细节，也会让脚步慢下来。',
          '把问题写在随手可见的地方。每次出门，只需要带着它走十分钟，不急着得到答案。',
        ],
      },
      {
        id: 'city-02',
        title: '看见日常的秩序',
        body: [
          '观察不只发生在地标旁。早餐摊收摊的时间、校门口的等待、树荫下停下的自行车，都在说明一处空间如何被人使用。',
          '选择一个固定的路口，在不同时间各停留一次。记录变化，而不是急着判断好坏。',
        ],
      },
      {
        id: 'city-03',
        title: '为一条街留下证据',
        body: [
          '第三次阅读之后，试着选择一条你已经走过的街。不要拍太多照片，先写下三个能证明它存在的细节：一种声音、一段气味、一个重复发生的动作。',
          '证据会让抽象的“喜欢”或“不喜欢”有落点。等你回头再读这些记录，城市会从目的地变成与你有关的生活现场。',
          '完成今天的阅读后，给行动留下一句自己的观察。它不需要完整，也不必适合发布。',
        ],
      },
      {
        id: 'city-04',
        title: '把路线画出来',
        body: [
          '路线不是最短距离，而是你愿意重复走的连接。把书店、菜场、公交站和一段安静的人行道放进同一张图里。',
          '下一次出门时，只修改其中一个点。小幅调整比重新规划更容易让观察持续。',
        ],
      },
      {
        id: 'city-05',
        title: '比较两种节奏',
        body: [
          '同一段路在工作日和休息日，会露出不同的性格。比较人群、停留时间和声音的密度，看看什么让你感到被催促，什么让你愿意多停一会儿。',
          '把差异写成一句完整的话。这是从观看走向理解的一步。',
        ],
      },
      {
        id: 'city-06',
        title: '留下自己的城市方法',
        body: [
          '现在回看前面的记录：你会先问什么问题，会在哪些时刻停下，又会用什么方式留下证据？这就是一套属于你的城市观察方法。',
          '把它带到下一座城市，也带回今天的生活。阅读完成之后，路线还会继续生长。',
        ],
      },
    ],
  },
];
